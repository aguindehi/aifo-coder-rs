use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Re-run build script when this file changes
    println!("cargo:rerun-if-changed=build.rs");

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
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_DATE={}", build_date);

    // Target triple and profile
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_TARGET={}", target);

    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_PROFILE={}", profile);

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
    println!("cargo:rustc-env=AIFO_SHIM_BUILD_RUSTC={}", rustc_ver);
}
