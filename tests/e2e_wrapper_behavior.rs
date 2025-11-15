use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

/// Returns the repository root (where the wrapper script "aifo-coder" resides).
fn repo_root() -> PathBuf {
    // Cargo sets CARGO_MANIFEST_DIR for integration tests to the crate root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Create an executable stub named "aifo-coder" that:
// - exits 0 on "--version" (for can_run check)
// - prints a marker and exits 0 for normal execution
fn make_sysbin_stub(dir: &std::path::Path) -> PathBuf {
    let stub_path = dir.join("aifo-coder");
    let mut f = fs::File::create(&stub_path).expect("create stub");
    #[cfg(unix)]
    {
        let script = r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "stub version"
  exit 0
fi
echo "SYSBIN_OK"
exit 0
"#;
        f.write_all(script.as_bytes()).expect("write stub");
        let mut perms = fs::metadata(&stub_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub_path, perms).unwrap();
    }
    #[cfg(not(unix))]
    {
        // Windows: simple batch file
        let script = r#"
@echo off
IF "%1" == "--version" (
  echo stub version
  exit /B 0
)
echo SYSBIN_OK
exit /B 0
"#;
        f.write_all(script.as_bytes()).expect("write stub");
    }
    stub_path
}
#[ignore]
#[test]
fn e2e_wrapper_prefers_system_binary() {
    // Create a temporary directory to host our sysbin stub
    let td = tempfile::tempdir().expect("tmpdir");
    let stub_dir = td.path().to_path_buf();
    let _stub = make_sysbin_stub(&stub_dir);

    // Prepend stub dir to PATH so command -v aifo-coder finds our stub first
    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", stub_dir.display(), old_path);

    // Run the wrapper script; it should exec our stub and print SYSBIN_OK
    let root = repo_root();
    let wrapper = root.join("aifo-coder");

    let out = Command::new(wrapper)
        .arg("wrapper-pass")
        .env("PATH", &new_path)
        .output()
        .expect("run wrapper with stub sysbin");

    assert!(
        out.status.success(),
        "wrapper should exec system binary successfully"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("SYSBIN_OK"),
        "expected SYSBIN_OK from sysbin stub, got: {}",
        stdout
    );
}
#[ignore]
#[test]
fn e2e_wrapper_fallback_to_local_binary_or_build() {
    // Ensure no system-installed 'aifo-coder' takes precedence
    let old_path = env::var("PATH").unwrap_or_default();
    let new_path = old_path; // do not prepend any stub; rely on wrapper fallback

    // Invoke wrapper with --version; expect it to exec local binary (debug or release)
    let root = repo_root();
    let wrapper = root.join("aifo-coder");

    let out = Command::new(wrapper)
        .arg("--version")
        .env("PATH", &new_path)
        .output()
        .expect("run wrapper fallback");

    assert!(
        out.status.success(),
        "wrapper fallback should succeed (debug/release/cargo build); status: {:?}",
        out.status.code()
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")) || stdout.to_lowercase().contains("aifo-coder"),
        "expected version or program name in stdout, got: {}",
        stdout
    );
}
