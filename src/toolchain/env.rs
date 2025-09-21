use std::env;

/// Environment variables to pass through into sidecars (proxy/cargo/networking).
pub(crate) const PROXY_ENV_NAMES: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "CARGO_NET_GIT_FETCH_WITH_CLI",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
];

/// Env vars that must not be forwarded from host into sidecars to avoid
/// host-specific toolchain and cargo path interference.
pub(crate) const PROHIBITED_PASSTHROUGH_ENV: &[&str] = &[
    "RUSTUP_TOOLCHAIN",
    "RUSTUP_HOME",
    "CARGO_HOME",
    "CARGO_TARGET_DIR",
];

/// Push an environment variable (-e KEY=VAL) into docker args.
pub(crate) fn push_env(args: &mut Vec<String>, k: &str, v: &str) {
    args.push("-e".to_string());
    args.push(format!("{k}={v}"));
}

/// Pass through selected environment variables from host into docker args.
pub(crate) fn apply_passthrough_envs(args: &mut Vec<String>, keys: &[&str]) {
    for name in keys {
        // Do not forward host rustup/cargo environment into sidecars
        if PROHIBITED_PASSTHROUGH_ENV.contains(name) {
            continue;
        }
        if let Ok(val) = env::var(name) {
            if !val.is_empty() {
                push_env(args, name, &val);
            }
        }
    }
}

/// Apply Rust linker flags when AIFO_RUST_LINKER is set (lld/mold).
pub(crate) fn apply_rust_linker_flags_if_set(args: &mut Vec<String>) {
    if let Ok(linker) = env::var("AIFO_RUST_LINKER") {
        let lk = linker.to_ascii_lowercase();
        let extra = if lk == "lld" {
            Some("-Clinker=clang -Clink-arg=-fuse-ld=lld")
        } else if lk == "mold" {
            Some("-Clinker=clang -Clink-arg=-fuse-ld=mold")
        } else {
            None
        };
        if let Some(add) = extra {
            let base = env::var("RUSTFLAGS").ok().unwrap_or_default();
            let rf = if base.trim().is_empty() {
                add.to_string()
            } else {
                format!("{base} {add}")
            };
            push_env(args, "RUSTFLAGS", &rf);
        }
    }
}

/// Apply normative Rust environment variables (do not override PATH).
pub(crate) fn apply_rust_common_env(args: &mut Vec<String>) {
    // Ensure host toolchain selection doesn't leak into the container
    push_env(args, "RUSTUP_TOOLCHAIN", "");

    push_env(args, "CARGO_HOME", "/home/coder/.cargo");
    push_env(args, "CC", "gcc");
    push_env(args, "CXX", "g++");
    let rb = env::var("RUST_BACKTRACE").ok();
    if rb.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
        push_env(args, "RUST_BACKTRACE", "1");
    }
}
