/*
   E2E tests for sccache within the macOS cross image.
   These tests must run inside the aifo-coder-macos-cross-rust-builder image.
   They are marked #[ignore] and executed explicitly in CI.
*/

// ignore-tidy-linelength

use std::fs;
use std::process::{Command, Stdio};
#[path = "support/mod.rs"]
mod support;
use support::should_run_macos_cross;

fn run_sh(cmd: &str, cwd: Option<&std::path::Path>) -> (i32, String, String) {
    let mut c = Command::new("sh");
    c.arg("-lc")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
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

fn parse_compile_requests(stats_out: &str) -> u64 {
    for line in stats_out.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("compile requests") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(last) = parts.last() {
                if let Ok(n) = last.parse::<u64>() {
                    return n;
                }
            }
        }
    }
    0
}

fn parse_cache_hits(stats_out: &str) -> u64 {
    for line in stats_out.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("cache hits") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(last) = parts.last() {
                if let Ok(n) = last.parse::<u64>() {
                    return n;
                }
            }
        }
    }
    0
}

fn parse_cache_misses(stats_out: &str) -> u64 {
    for line in stats_out.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("cache misses") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(last) = parts.last() {
                if let Ok(n) = last.parse::<u64>() {
                    return n;
                }
            }
        }
    }
    0
}

fn parse_non_cacheable_calls(stats_out: &str) -> u64 {
    for line in stats_out.lines() {
        let line = line.trim();
        if line.to_lowercase().starts_with("non-cacheable calls") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(last) = parts.last() {
                if let Ok(n) = last.parse::<u64>() {
                    return n;
                }
            }
        }
    }
    0
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

#[test]
#[ignore]
fn e2e_macos_cross_sccache_available() {
    if !should_run_macos_cross() {
        eprintln!("skipping: not macos-cross environment");
        return;
    }
    // Ensure sccache CLI is present and can show stats (server may auto-start).
    require_tool("sccache");
    let (vcode, vout, verr) = run_sh("sccache --version", None);
    assert_eq!(vcode, 0, "sccache --version failed: {}\n{}", vout, verr);

    // Try starting server and showing stats; tolerate existing server.
    let (_scode, _sout, _serr) = run_sh("sccache --start-server || true", None);
    let (code, out, err) = run_sh("sccache --show-stats", None);
    assert_eq!(code, 0, "sccache --show-stats failed: {}\n{}", out, err);
    assert!(
        out.to_lowercase().contains("compile requests")
            || out.to_lowercase().contains("cache hits")
            || out.to_lowercase().contains("cache misses"),
        "unexpected stats output:\n{}",
        out
    );
}

#[test]
#[ignore]
fn e2e_macos_cross_sccache_used() {
    if !should_run_macos_cross() {
        eprintln!("skipping: not macos-cross environment");
        return;
    }
    // Prepare clean stats
    let (_st1, _o1, _e1) = run_sh("sccache --start-server || true", None);
    let (_zt, _oz, _ez) = run_sh("sccache --zero-stats || true", None);

    // Create a tiny crate
    let dir = tempfile::tempdir().expect("tmpdir");
    let root = dir.path().to_path_buf();
    create_min_crate(&root, "hello");

    // Discover SDK dir
    let (sdk_code, sdk_out, sdk_err) = run_sh(
        "cat /opt/osxcross/SDK/SDK_DIR.txt 2>/dev/null \
         || ls -d /opt/osxcross/target/SDK/MacOSX*.sdk 2>/dev/null | head -n1",
        None,
    );
    assert_eq!(
        sdk_code, 0,
        "cannot discover SDK dir for Rust build: {}\n{}",
        sdk_out, sdk_err
    );
    let sdk = sdk_out.trim();

    // Build for aarch64-apple-darwin with explicit linker (env inherited uses sccache wrapper)
    // Ensure target is installed (best-effort; avoids CA flakiness if image pre-install failed)
    let (_tcode, _tout, _terr) = run_sh(
        "/usr/local/cargo/bin/rustup target add aarch64-apple-darwin || true",
        Some(&root),
    );
    let cmd = format!(
        "SDKROOT='{}' OSX_SYSROOT='{}' \
         CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER='/opt/osxcross/target/bin/oa64-clang' \
         /usr/local/cargo/bin/cargo build --release --target aarch64-apple-darwin",
        sdk, sdk
    );
    let (bcode, bout, berr) = run_sh(&cmd, Some(&root));
    assert_eq!(
        bcode, 0,
        "cargo build failed.\nstdout:\n{}\nstderr:\n{}",
        bout, berr
    );

    // Stats should show at least one compile request
    let (scode, sout, serr) = run_sh("sccache --show-stats", None);
    assert_eq!(scode, 0, "sccache --show-stats failed: {}\n{}", sout, serr);
    let reqs_first = parse_compile_requests(&sout);
    assert!(
        reqs_first > 0,
        "expected compile requests > 0, got {}.\nStats:\n{}",
        reqs_first,
        sout
    );

    // Force a recompile to exercise cache hits on identical inputs
    let (_clcode, _clout, _clerr) = run_sh("/usr/local/cargo/bin/cargo clean", Some(&root));
    let (b2code, b2out, b2err) = run_sh(&cmd, Some(&root));
    assert_eq!(
        b2code, 0,
        "second cargo build failed.\nstdout:\n{}\nstderr:\n{}",
        b2out, b2err
    );

    // After second build, expect cache hits > 0
    let (scode2, sout2, serr2) = run_sh("sccache --show-stats", None);
    assert_eq!(
        scode2, 0,
        "sccache --show-stats failed: {}\n{}",
        sout2, serr2
    );
    let hits = parse_cache_hits(&sout2);
    let misses = parse_cache_misses(&sout2);
    let nccalls = parse_non_cacheable_calls(&sout2);
    if std::env::var("AIFO_SCCACHE_EXPECT_HITS").ok().as_deref() == Some("1") {
        assert!(
            hits > 0,
            "expected cache hits > 0 after second identical build (strict mode).\nStats:\n{}",
            sout2
        );
    } else {
        assert!(
            hits > 0 || misses > 0 || nccalls > 0,
            "expected sccache activity (hits/misses/non-cacheable) after second build; got hits={}, misses={}, non-cacheable={}.\nStats:\n{}",
            hits, misses, nccalls, sout2
        );
    }
}
