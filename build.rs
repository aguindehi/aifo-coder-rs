use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Re-run build script when this file changes
    println!("cargo:rerun-if-changed=build.rs");

    // Optional: bake in a default OTLP endpoint and transport for telemetry from external configuration.
    // Priority (endpoint and transport):
    //   1) AIFO_OTEL_ENDPOINT_FILE: first non-empty line = endpoint, second non-empty line (optional) = transport
    //   2) AIFO_OTEL_ENDPOINT (env value) + optional AIFO_OTEL_TRANSPORT (env value)
    if let Ok(path) = std::env::var("AIFO_OTEL_ENDPOINT_FILE") {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let mut lines = contents
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty());
            if let Some(ep) = lines.next() {
                println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={ep}");
            }
            if let Some(transport) = lines.next() {
                let t = transport.trim().to_ascii_lowercase();
                if t == "grpc" || t == "http" {
                    println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_TRANSPORT={t}");
                }
            }
        }
    } else if let Ok(val) = std::env::var("AIFO_OTEL_ENDPOINT") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={trimmed}");
        }
        if let Ok(t) = std::env::var("AIFO_OTEL_TRANSPORT") {
            let tl = t.trim().to_ascii_lowercase();
            if tl == "grpc" || tl == "http" {
                println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_TRANSPORT={tl}");
            }
        }
    }

    // Build date (UTC ISO-8601). Fallback to unix:<secs> if `date` is unavailable.
    let build_date = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| std::time::Duration::from_secs(0))
                .as_secs();
            format!("unix:{secs}")
        });
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_DATE={build_date}");

    // Target triple and profile
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_TARGET={target}");

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_PROFILE={profile}");

    // rustc version (best-effort)
    let rustc_ver = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_RUSTC={rustc_ver}");
}
