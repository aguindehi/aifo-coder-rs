use rustc_version::version;
use time::format_description::well_known::Iso8601;
use time::OffsetDateTime;

fn sanitize_env_value(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains('\n') || s.contains('\r') || s.contains('\0') {
        return None;
    }
    Some(s.to_string())
}

fn main() {
    // Re-run build script when this file changes
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=AIFO_OTEL_ENDPOINT_FILE");
    println!("cargo:rerun-if-env-changed=AIFO_OTEL_ENDPOINT");
    println!("cargo:rerun-if-env-changed=AIFO_OTEL_TRANSPORT");
    println!("cargo:rerun-if-env-changed=TARGET");
    println!("cargo:rerun-if-env-changed=PROFILE");

    // Optional: bake in a default OTLP endpoint and transport for telemetry from external configuration.
    // Priority (endpoint and transport):
    //   1) AIFO_OTEL_ENDPOINT_FILE: first non-empty line = endpoint, second non-empty line (optional) = transport
    //   2) AIFO_OTEL_ENDPOINT (env value) + optional AIFO_OTEL_TRANSPORT (env value)
    if let Ok(path) = std::env::var("AIFO_OTEL_ENDPOINT_FILE") {
        println!("cargo:rerun-if-changed={path}");
        if let Ok(contents) = std::fs::read_to_string(&path) {
            let mut lines = contents.lines().map(|l| l.trim()).filter(|l| !l.is_empty());
            if let Some(ep) = lines.next().and_then(sanitize_env_value) {
                println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={ep}");
            }
            if let Some(transport) = lines.next().and_then(sanitize_env_value) {
                let t = transport.trim().to_ascii_lowercase();
                if t == "grpc" || t == "http" {
                    println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_TRANSPORT={t}");
                }
            }
        }
    } else if let Ok(val) = std::env::var("AIFO_OTEL_ENDPOINT") {
        if let Some(trimmed) = sanitize_env_value(&val) {
            println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_ENDPOINT={trimmed}");
        }
        if let Ok(t) = std::env::var("AIFO_OTEL_TRANSPORT") {
            if let Some(tl0) = sanitize_env_value(&t) {
                let tl = tl0.trim().to_ascii_lowercase();
                if tl == "grpc" || tl == "http" {
                    println!("cargo:rustc-env=AIFO_OTEL_DEFAULT_TRANSPORT={tl}");
                }
            }
        }
    }

    // Build date (UTC ISO-8601).
    let now = OffsetDateTime::now_utc();
    let build_date = now
        .format(&Iso8601::DEFAULT)
        .unwrap_or_else(|_| format!("unix:{}", now.unix_timestamp()));
    if let Some(v) = sanitize_env_value(&build_date) {
        println!("cargo:rustc-env=AIFO_SHIM_BUILD_DATE={v}");
    }

    // Target triple and profile
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    if let Some(v) = sanitize_env_value(&target) {
        println!("cargo:rustc-env=AIFO_SHIM_BUILD_TARGET={v}");
    }

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    if let Some(v) = sanitize_env_value(&profile) {
        println!("cargo:rustc-env=AIFO_SHIM_BUILD_PROFILE={v}");
    }

    // rustc version (best-effort)
    let rustc_ver = version()
        .map(|v| v.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    if let Some(v) = sanitize_env_value(&rustc_ver) {
        println!("cargo:rustc-env=AIFO_SHIM_BUILD_RUSTC={v}");
    }
}
