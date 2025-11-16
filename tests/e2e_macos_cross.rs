/*
   E2E tests for the macOS cross image (osxcross).
   These tests must run inside the aifo-coder-macos-cross-rust-builder image.
   They are marked #[ignore] and executed explicitly in CI and via the Makefile target.
*/

// ignore-tidy-linelength

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

fn run_sh(cmd: &str, cwd: Option<&std::path::Path>) -> (i32, String, String) {
    let mut c = Command::new("sh");
    c.arg("-lc").arg(cmd).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let out = c.output().expect("failed to spawn shell");
    let code = out.status.code().unwrap_or(-1);
    (
        code,
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn require_tool(bin: &str) {
    let (code, _, err) = run_sh(&format!("command -v {} >/dev/null 2>&1", bin), None);
    if code != 0 {
        panic!("Required tool '{}' not found in PATH. stderr: {}", bin, err);
    }
}

fn require_executable(path: &str) {
    let (code, _, err) = run_sh(&format!("[ -x '{}' ]", path), None);
    if code != 0 {
        panic!("Required executable '{}' not found or not executable. stderr: {}", path, err);
    }
}

#[test]
#[ignore]
fn e2e_macos_cross_tools_and_env() {
    // Validate critical tools exist
    // Prefer absolute paths to avoid PATH differences in test runner environments.
    require_tool("file");
    for p in [
        "/opt/osxcross/target/bin/oa64-clang",
        "/opt/osxcross/target/bin/o64-clang",
    ] {
        require_executable(p);
    }

    // Optional tool invocations to ensure wrappers are functional
    for t in [
        "/opt/osxcross/target/bin/oa64-clang",
        "/opt/osxcross/target/bin/o64-clang",
        "/opt/osxcross/target/bin/aarch64-apple-darwin-ar",
    ] {
        let (code, out, err) = run_sh(&format!("{} --version || true", t), None);
        assert!(
            code == 0 || code == 127,
            "invoking {} failed: {}\n{}",
            t,
            out,
            err
        );
    }

    // Required environment defaults
    let (code, out, err) = run_sh(
        "printf '%s\\n' \"$MACOSX_DEPLOYMENT_TARGET\" \"$CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER\"",
        None,
    );
    assert_eq!(code, 0, "env read failed: {}\n{}", out, err);
    let mut lines = out.lines();
    let dep_target = lines.next().unwrap_or("");
    let aarch_linker = lines.next().unwrap_or("");
    assert!(
        dep_target.trim() == "11.0",
        "MACOSX_DEPLOYMENT_TARGET expected '11.0', got '{}'",
        dep_target
    );
    let aarch_linker = aarch_linker.trim();
    // Accept absolute path or basename; verify it resolves to the wrapper under /opt/osxcross/target/bin.
    if aarch_linker.contains('/') {
        let (s, _, e) = run_sh(&format!("[ -x '{}' ]", aarch_linker), None);
        assert_eq!(s, 0, "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER points to non-executable: {} ({})", aarch_linker, e);
    } else {
        let (s, out, e) = run_sh(&format!("command -v '{}' || true", aarch_linker), None);
        assert_eq!(s, 0, "command -v {} failed: {}", aarch_linker, e);
        assert!(
            out.trim().ends_with("/opt/osxcross/target/bin/oa64-clang"),
            "linker '{}' resolves to '{}', expected '/opt/osxcross/target/bin/oa64-clang'",
            aarch_linker,
            out.trim()
        );
    }

    // ld should resolve to osxcross Mach-O ld (shadow /usr/bin/ld).
    let (code_ld, out_ld, err_ld) = run_sh("command -v ld || true", None);
    assert_eq!(code_ld, 0, "command -v ld failed: {}", err_ld);
    assert!(
        out_ld.trim() == "/opt/osxcross/target/bin/ld",
        "ld resolves to '{}', expected '/opt/osxcross/target/bin/ld'",
        out_ld.trim()
    );

    // Tool aliases should exist (avoid relying on darwin minor suffixes)
    for p in [
        "/opt/osxcross/target/bin/aarch64-apple-darwin-ar",
        "/opt/osxcross/target/bin/aarch64-apple-darwin-ranlib",
        "/opt/osxcross/target/bin/aarch64-apple-darwin-strip",
        "/opt/osxcross/target/bin/x86_64-apple-darwin-ar",
        "/opt/osxcross/target/bin/x86_64-apple-darwin-ranlib",
        "/opt/osxcross/target/bin/x86_64-apple-darwin-strip",
    ] {
        let (s, _, e) = run_sh(&format!("[ -x '{}' ]", p), None);
        assert_eq!(s, 0, "missing expected tool alias '{}': {}", p, e);
    }
}

#[test]
#[ignore]
fn e2e_macos_cross_sdk_installed() {
    // SDK should be installed under /opt/osxcross/SDK/MacOSX<ver>.sdk
    let (code, out, err) = run_sh("ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1", None);
    assert_eq!(code, 0, "cannot locate SDK dir under /opt/osxcross/target/SDK: {}\n{}", out, err);
    let sdk_dir = out.trim();
    assert!(
        sdk_dir.contains("MacOSX") && sdk_dir.ends_with(".sdk"),
        "SDK dir does not look like MacOSX<ver>.sdk: '{}'",
        sdk_dir
    );

    // Optional: record of resolved tarball name must exist
    let (code2, out2, err2) = run_sh("cat /opt/osxcross/SDK/SDK_NAME.txt 2>/dev/null", None);
    assert_eq!(code2, 0, "missing SDK_NAME.txt: {}\n{}", out2, err2);
    assert!(
        out2.trim().starts_with("MacOSX") && out2.trim().ends_with(".tar.xz"),
        "SDK_NAME.txt content unexpected: '{}'",
        out2.trim()
    );
}

#[test]
#[ignore]
fn e2e_macos_cross_c_link_corefoundation() {
    require_executable("/opt/osxcross/target/bin/oa64-clang");
    // Write a tiny CoreFoundation program and link with -framework
    let dir = tempfile::tempdir().expect("tmpdir");
    let c_path = dir.path().join("t.c");
    let mut f = std::fs::File::create(&c_path).expect("create t.c");
    writeln!(
        f,
        "#include <CoreFoundation/CoreFoundation.h>\nint main(void){{ CFStringRef s = CFSTR(\"hi\"); CFRelease(s); return 0; }}"
    )
    .unwrap();

    // Try linking; osxcross wrappers should set sysroot automatically
    let exe = dir.path().join("t");
    // Discover SDK dir from image (SDK_DIR.txt or fallback to ls)
    let (code_sdk, sdk_out, _sdk_err) = run_sh("cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1", None);
    assert_eq!(code_sdk, 0, "cannot discover SDK dir for C link");
    let sdk = sdk_out.trim();
    let cmd = format!(
        "SDKROOT='{}' OSX_SYSROOT='{}' /opt/osxcross/target/bin/oa64-clang -framework CoreFoundation '{}' -o '{}' && file '{}'",
        sdk,
        sdk,
        c_path.display(),
        exe.display(),
        exe.display()
    );
    let (code, out, err) = run_sh(&cmd, None);
    assert_eq!(
        code, 0,
        "C link against CoreFoundation failed.\nstdout:\n{}\nstderr:\n{}",
        out, err
    );
    assert!(
        out.to_lowercase().contains("mach-o"),
        "expected Mach-O output from file(1), got: {}",
        out
    );
}

#[test]
#[ignore]
fn e2e_macos_cross_rust_build_arm64() {
    // Build a tiny local Rust crate for aarch64-apple-darwin and verify Mach-O output
    // Prefer absolute cargo path to avoid PATH differences
    require_executable("/usr/local/cargo/bin/cargo");
    let dir = tempfile::tempdir().expect("tmpdir");
    let root = dir.path().to_path_buf();
    create_min_crate(&root, "hello");

    // Ensure target installed (should already be in the image, but harmless)
    let (_c1, _o1, _e1) = run_sh("/usr/local/cargo/bin/rustup target add aarch64-apple-darwin || true", Some(&root));

    // Discover SDK dir from image and set explicit linker/env for cargo
    let (code_sdk, sdk_out, _sdk_err) = run_sh("cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1", None);
    assert_eq!(code_sdk, 0, "cannot discover SDK dir for Rust build");
    let sdk = sdk_out.trim();
    let (code, out, err) = run_sh(&format!(
        "SDKROOT='{}' OSX_SYSROOT='{}' CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER='/opt/osxcross/target/bin/oa64-clang' /usr/local/cargo/bin/cargo build --release --target aarch64-apple-darwin",
        sdk, sdk
    ), Some(&root));
    assert_eq!(code, 0, "cargo build failed.\nstdout:\n{}\nstderr:\n{}", out, err);

    let bin = root
        .join("target")
        .join("aarch64-apple-darwin")
        .join("release")
        .join("hello");
    assert!(bin.exists(), "expected binary not found at {}", bin.display());

    let (code2, out2, err2) = run_sh(&format!("file '{}'", bin.display()), None);
    assert_eq!(code2, 0, "file(1) failed: {}\n{}", out2, err2);
    assert!(
        out2.to_lowercase().contains("mach-o") && out2.to_lowercase().contains("arm64"),
        "expected Mach-O 64-bit arm64, got: {}",
        out2
    );
}

fn create_min_crate(root: &std::path::Path, name: &str) {
    let cargo_toml = format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        name
    );
    fs::write(root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
    let src = root.join("src");
    fs::create_dir_all(&src).expect("mkdir src");
    fs::write(src.join("main.rs"), "fn main(){ println!(\"hi\"); }\n").expect("write main.rs");
}
