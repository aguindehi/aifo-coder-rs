use std::env;
use std::fs;
use std::path::PathBuf;

fn write_file(p: &PathBuf, s: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).expect("mkdirs");
    }
    fs::write(p, s).expect("write");
}

#[test]
#[ignore] // E2E: runs real docker flows; enable in CI lanes intentionally
fn acceptance_rust_sidecar_smoke() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prefer our toolchain image; allow override via env
    let image = env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-rust-toolchain:latest".to_string());

    // Run a few basic commands inside the rust sidecar; ensure they succeed.
    // These are lighter than a full project test but catch image/runtime regressions.
    for cmd in [
        vec!["cargo".to_string(), "--version".to_string()],
        vec!["rustc".to_string(), "--version".to_string()],
        vec!["cargo".to_string(), "nextest".to_string(), "-V".to_string()],
    ] {
        let code = aifo_coder::toolchain_run(
            "rust",
            &cmd,
            Some(&image),
            false, // allow caches
            false,
            false,
        )
        .expect("toolchain_run returned io::Result");
        assert_eq!(
            code, 0,
            "command {:?} failed inside rust sidecar (image={})",
            cmd, image
        );
    }
}

#[test]
#[ignore] // E2E: full acceptance per Phase 8; runs real docker flows in a temp project
fn toolchain_rust_acceptance_full_suite() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prefer our toolchain image; allow override via env
    let image = env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-rust-toolchain:latest".to_string());

    // Allow more time in CI for the first run to build deps
    let old_to = env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS").ok();
    env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", "600");

    // Create a minimal Rust project in a temp workspace
    let td = tempfile::tempdir().expect("tmpdir");
    let ws = td.path().to_path_buf();

    let cargo_toml = r#"[package]
name = "aifo_phase8_smoke"
version = "0.1.0"
edition = "2021"

[dependencies]
"#;
    write_file(&ws.join("Cargo.toml"), cargo_toml);

    // Keep formatting simple and clippy-clean
    let lib_rs = r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adds() {
        assert_eq!(add(2, 3), 5);
    }
}
"#;
    write_file(&ws.join("src").join("lib.rs"), lib_rs);

    // Ensure repo-root detection works (many functions look for a .git directory)
    std::fs::create_dir_all(ws.join(".git")).expect("mkdir .git");

    // Change into workspace for sidecar execs
    let old_cwd = env::current_dir().expect("cwd");
    env::set_current_dir(&ws).expect("chdir");

    let run = |argv: &[&str]| -> i32 {
        let args: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        aifo_coder::toolchain_run("rust", &args, Some(&image), false, false, false)
            .expect("toolchain_run returned io::Result")
    };

    // Format check (rustfmt present in aifo image)
    let code_fmt = run(&[
        "cargo",
        "fmt",
        "--manifest-path",
        "Cargo.toml",
        "--",
        "--check",
    ]);
    assert_eq!(
        code_fmt, 0,
        "cargo fmt -- --check failed in rust sidecar (image={})",
        image
    );

    // Clippy (deny warnings)
    let code_clippy = run(&[
        "cargo",
        "clippy",
        "--manifest-path",
        "Cargo.toml",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ]);
    assert_eq!(
        code_clippy, 0,
        "cargo clippy -D warnings failed in rust sidecar (image={})",
        image
    );

    // cargo test
    let code_test = run(&[
        "cargo",
        "test",
        "--manifest-path",
        "Cargo.toml",
        "--no-fail-fast",
    ]);
    assert_eq!(
        code_test, 0,
        "cargo test failed in rust sidecar (image={})",
        image
    );

    // cargo nextest run
    let code_nextest = run(&[
        "cargo",
        "nextest",
        "run",
        "--manifest-path",
        "Cargo.toml",
        "--no-fail-fast",
    ]);
    assert_eq!(
        code_nextest, 0,
        "cargo nextest run failed in rust sidecar (image={})",
        image
    );

    // Restore env and cwd
    if let Some(v) = old_to {
        env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", v);
    } else {
        env::remove_var("AIFO_TOOLEEXEC_TIMEOUT_SECS");
    }
    env::set_current_dir(old_cwd).ok();
}
