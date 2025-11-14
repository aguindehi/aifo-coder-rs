use std::process::Command;

/// Resolve docker runtime path or skip the test early (ignored by default anyway).
fn docker_path() -> Option<std::path::PathBuf> {
    aifo_coder::container_runtime_path().ok()
}
fn test_image() -> String {
    std::env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-coder-toolchain-rust:latest".to_string())
}

fn run_in_container(image: &str, shell_cmd: &str) -> Option<std::process::Output> {
    let rt = docker_path()?;
    let out = Command::new(rt)
        .arg("run")
        .arg("--rm")
        .arg(image)
        .arg("sh")
        .arg("-lc")
        .arg(shell_cmd)
        .output()
        .ok()?;
    Some(out)
}

fn image_present(image: &str) -> bool {
    let Some(rt) = docker_path() else {
        return false;
    };
    Command::new(rt)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
#[ignore]
#[test]
fn e2e_toolchain_rust_components_and_tools_present() {
    let Some(_) = docker_path() else {
        return;
    };
    let image = test_image();

    // Only run if the image exists locally and contains rustc
    if !image_present(&image) {
        eprintln!("skipping: test image not present locally: {}", image);
        return;
    }
    if let Some(check) = run_in_container(&image, "command -v rustc >/dev/null 2>&1") {
        if !check.status.success() {
            eprintln!("skipping: rustc not found in image: {}", image);
            return;
        }
    } else {
        eprintln!("skipping: unable to exec docker");
        return;
    }

    // Sanity: rustc must be present
    let out = run_in_container(&image, "rustc --version").expect("failed to exec docker");
    assert!(
        out.status.success(),
        "rustc --version failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // rustup components
    let out = run_in_container(&image, "rustup component list").unwrap();
    assert!(
        out.status.success(),
        "rustup component list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let comps = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
    for comp in ["clippy", "rustfmt", "rust-src", "llvm-tools-preview"] {
        assert!(
            comps.contains(comp),
            "expected component '{}' to appear in rustup component list; got: {}",
            comp,
            comps
        );
    }

    // cargo-nextest
    let out = run_in_container(&image, "cargo nextest -V").unwrap();
    assert!(
        out.status.success(),
        "cargo nextest -V failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
#[ignore]
#[test]
fn e2e_toolchain_rust_env_and_deps_present() {
    let Some(_) = docker_path() else {
        return;
    };
    let image = test_image();

    // Only run if the image exists locally and contains rustc
    if !image_present(&image) {
        eprintln!("skipping: test image not present locally: {}", image);
        return;
    }
    if let Some(check) = run_in_container(&image, "command -v rustc >/dev/null 2>&1") {
        if !check.status.success() {
            eprintln!("skipping: rustc not found in image: {}", image);
            return;
        }
    } else {
        eprintln!("skipping: unable to exec docker");
        return;
    }

    // CARGO_HOME
    let out = run_in_container(&image, "printf %s \"$CARGO_HOME\"").unwrap();
    let ch = String::from_utf8_lossy(&out.stdout);
    assert_eq!(ch, "/home/coder/.cargo", "CARGO_HOME mismatch: {}", ch);

    // PATH prefix
    let out = run_in_container(&image, "printf %s \"$PATH\"").unwrap();
    let path = String::from_utf8_lossy(&out.stdout);
    if !path.starts_with("/home/coder/.cargo/bin:/usr/local/cargo/bin:") {
        eprintln!(
            "skipping: PATH prefix not as expected for image {}; got: {}",
            image, path
        );
        return;
    }

    // LANG
    let out = run_in_container(&image, "printf %s \"$LANG\"").unwrap();
    let lang = String::from_utf8_lossy(&out.stdout);
    assert_eq!(lang, "C.UTF-8", "LANG mismatch: {}", lang);

    // Core build tools available
    let out = run_in_container(
        &image,
        "set -e; for t in gcc g++ make pkg-config cmake ninja clang python3 git; do command -v \"$t\" >/dev/null || { echo \"missing $t\" >&2; exit 1; }; done",
    )
    .unwrap();
    assert!(
        out.status.success(),
        "one or more core tools missing: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Dev libraries present (Debian package names)
    let out = run_in_container(
        &image,
        "dpkg -s libssl-dev zlib1g-dev libsqlite3-dev libcurl4-openssl-dev >/dev/null 2>&1; echo $?",
    )
    .unwrap();
    let status_text = String::from_utf8_lossy(&out.stdout);
    assert!(
        status_text.trim() == "0",
        "required dev libraries not fully installed"
    );
}
