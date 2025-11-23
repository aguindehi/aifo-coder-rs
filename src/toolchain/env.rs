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
    // Preview/env policy flags: pass through to sidecars and preview runs
    "AIFO_CONFIG_ALLOW_EXT",
    "AIFO_CONFIG_COPY_ALWAYS",
    "AIFO_CONFIG_MAX_SIZE",
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
    // Ensure host toolchain selection doesn't leak into the container (blocked by PROHIBITED_PASSTHROUGH_ENV).
    // Default: prefer stable toolchain pinned to image's RUSTUP_HOME for determinism.
    // Opt-in developer mode: when AIFO_CODER_RUSTUP_MUTABLE=1, make rustup writable for on-demand installs.
    let rustup_mutable = env::var("AIFO_CODER_RUSTUP_MUTABLE").ok().as_deref() == Some("1");
    // On official rust images (or when explicitly requested), avoid forcing RUSTUP_TOOLCHAIN to prevent rustup sync noise.
    let official = env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok().as_deref() == Some("1")
        || env::var("AIFO_RUST_TOOLCHAIN_USE_OFFICIAL").ok().as_deref() == Some("1");

    if rustup_mutable {
        // Developer mode: use per-user rustup home; do not force RUSTUP_TOOLCHAIN to allow switching (e.g., nightly).
        push_env(args, "RUSTUP_HOME", "/home/coder/.rustup");
    } else if env::var("AIFO_RUST_OFFICIAL_BOOTSTRAP").ok().as_deref() == Some("1") {
        // Official rust:<ver> image: use system rustup home and do not force a moving channel.
        // This reduces rustup channel sync chatter on first use.
        push_env(args, "RUSTUP_HOME", "/usr/local/rustup");
    } else {
        // Deterministic mode: use system rustup home; only force stable when not using official images.
        if !official {
            push_env(args, "RUSTUP_TOOLCHAIN", "stable");
        }
        push_env(args, "RUSTUP_HOME", "/usr/local/rustup");
    }

    // Cargo home remains per-user for writable caches/tools (overridden as needed for official images elsewhere).
    push_env(args, "CARGO_HOME", "/home/coder/.cargo");

    // Linker defaults
    push_env(args, "CC", "gcc");
    push_env(args, "CXX", "g++");

    // Enable backtraces unless explicitly set
    let rb = env::var("RUST_BACKTRACE").ok();
    if rb.as_deref().map(|s| s.is_empty()).unwrap_or(true) {
        push_env(args, "RUST_BACKTRACE", "1");
    }
}
