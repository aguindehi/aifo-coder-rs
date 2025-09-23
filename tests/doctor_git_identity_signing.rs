//// ignore-tidy-linelength

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn capture_stderr<F: FnOnce()>(f: F) -> String {
    use libc::{dup, dup2, fflush, fileno, fopen, STDERR_FILENO};
    use std::os::fd::RawFd;

    unsafe {
        // Open a temporary file
        let path = std::ffi::CString::new("/tmp/aifo-coder-test-stderr-doctor.tmp").unwrap();
        let mode = std::ffi::CString::new("w+").unwrap();
        let file = fopen(path.as_ptr(), mode.as_ptr());
        assert!(!file.is_null(), "failed to open temp file for capture");
        let fd: RawFd = fileno(file);

        // Save the original stderr fd and redirect to our temp file
        let old_fd = dup(STDERR_FILENO);
        assert!(old_fd >= 0, "dup(STDERR_FILENO) failed");

        assert!(
            dup2(fd, STDERR_FILENO) >= 0,
            "dup2(temp, STDERR_FILENO) failed"
        );

        // Execute the function while stderr is redirected
        f();

        // Flush buffers and restore stderr
        fflush(file);
        assert!(dup2(old_fd, STDERR_FILENO) >= 0, "restore stderr failed");
        let _ = libc::close(old_fd);
    }

    // Read captured output
    std::fs::read_to_string("/tmp/aifo-coder-test-stderr-doctor.tmp")
        .unwrap_or_else(|_| String::new())
}

fn write_global_gitconfig(p: &PathBuf, content: &str) {
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(p, content).expect("write global gitconfig");
}

fn run_git_in(dir: &PathBuf, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git command failed: {:?}", args);
}

#[test]
fn test_doctor_identity_precedence_and_signing_repo_over_global() {
    if !git_available() {
        eprintln!("skipping: git not found in PATH");
        return;
    }

    // Isolate HOME and global gitconfig
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    env::set_var("HOME", &home);

    let global_cfg = home.join(".gitconfig-global");
    env::set_var("GIT_CONFIG_GLOBAL", &global_cfg);

    write_global_gitconfig(
        &global_cfg,
        "[user]\n\tname = Global Name\n\temail = global@example.com\n[commit]\n\tgpgsign = true\n",
    );

    // Create isolated repo and set repo-specific config
    let repo = home.join("repo");
    fs::create_dir_all(&repo).expect("mkdir repo");
    run_git_in(&repo, &["init"]);
    run_git_in(&repo, &["config", "user.name", "Repo Name"]);
    run_git_in(&repo, &["config", "user.email", "repo@example.com"]);
    run_git_in(&repo, &["config", "commit.gpgsign", "false"]);

    // Set env overrides which must take highest precedence
    env::set_var("GIT_AUTHOR_NAME", "Env Name");
    env::set_var("GIT_AUTHOR_EMAIL", "env@example.com");

    // Ensure we run doctor inside the repo
    std::env::set_current_dir(&repo).expect("chdir repo");

    let output = capture_stderr(|| {
        aifo_coder::run_doctor(false);
    });

    // Identity precedence: env > repo > global
    assert!(
        output.contains("repo user.name") && output.contains("✅ set"),
        "repo user.name should be reported as set; output:\n{}",
        output
    );
    assert!(
        output.contains("global user.name") && output.contains("✅ set"),
        "global user.name should be reported as set; output:\n{}",
        output
    );
    assert!(
        output.contains("effective author name") && output.contains("Env Name"),
        "effective author name should include env value; output:\n{}",
        output
    );

    assert!(
        output.contains("repo user.email") && output.contains("✅ set"),
        "repo user.email should be reported as set; output:\n{}",
        output
    );
    assert!(
        output.contains("global user.email") && output.contains("✅ set"),
        "global user.email should be reported as set; output:\n{}",
        output
    );
    assert!(
        output.contains("effective author email") && output.contains("env@example.com"),
        "effective author email should include env value; output:\n{}",
        output
    );

    // Signing precedence: repo > global
    assert!(
        output.contains("commit.gpgsign (repo)") && output.contains("false"),
        "repo commit.gpgsign should be false; output:\n{}",
        output
    );
    assert!(
        output.contains("commit.gpgsign (global)") && output.contains("true"),
        "global commit.gpgsign should be true; output:\n{}",
        output
    );
    assert!(
        output.contains("commit.gpgsign (effective)") && output.contains("false"),
        "effective commit.gpgsign should be false due to repo precedence; output:\n{}",
        output
    );
}

#[test]
fn test_doctor_verbose_tips_when_desired_off_but_repo_enables_signing() {
    if !git_available() {
        eprintln!("skipping: git not found in PATH");
        return;
    }

    // Isolate HOME and global gitconfig
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    env::set_var("HOME", &home);

    let global_cfg = home.join(".gitconfig-global");
    env::set_var("GIT_CONFIG_GLOBAL", &global_cfg);

    write_global_gitconfig(&global_cfg, "[commit]\n\tgpgsign = false\n");

    // Create isolated repo and enable repo signing
    let repo = home.join("repo");
    fs::create_dir_all(&repo).expect("mkdir repo");
    run_git_in(&repo, &["init"]);
    run_git_in(&repo, &["config", "user.name", "Repo Name"]);
    run_git_in(&repo, &["config", "user.email", "repo@example.com"]);
    run_git_in(&repo, &["config", "commit.gpgsign", "true"]);

    // Desired signing disabled
    env::set_var("AIFO_CODER_GIT_SIGN", "0");

    // Ensure we run doctor inside the repo
    std::env::set_current_dir(&repo).expect("chdir repo");

    let output = capture_stderr(|| {
        aifo_coder::run_doctor(true);
    });

    // Tip should be printed when desired_signing=false but repo enables signing
    assert!(
        output.contains("Signing disabled by AIFO_CODER_GIT_SIGN=0 but repo enables it"),
        "doctor verbose tips should include disabled-signing tip; output:\n{}",
        output
    );
    assert!(
        output.contains("git config commit.gpgsign false"),
        "doctor verbose tips should suggest disabling signing; output:\n{}",
        output
    );
}
