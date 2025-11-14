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

fn run_doctor_capture(verbose: bool) -> String {
    // Resolve wrapper path from the project root regardless of current directory.
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wrapper = {
        let p = project_root.join("aifo-coder");
        #[cfg(windows)]
        {
            p.with_extension("exe")
        }
        #[cfg(not(windows))]
        {
            p
        }
    };

    if verbose {
        env::set_var("AIFO_CODER_DOCTOR_VERBOSE", "1");
    }
    let mut cmd = Command::new(wrapper);
    cmd.arg("doctor");
    let output = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("run doctor");
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[test]
fn int_doctor_identity_precedence_and_signing_repo_over_global() {
    if !git_available() {
        eprintln!("skipping: git not found in PATH");
        return;
    }

    // Isolate HOME and global gitconfig
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    env::set_var("HOME", &home);

    let global_cfg = home.join(".gitconfig");
    env::set_var("GIT_CONFIG_GLOBAL", &global_cfg);
    env::set_var("GIT_CONFIG_NOSYSTEM", "1");

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

    let output = run_doctor_capture(false);

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

    // Cleanup env overrides
    env::remove_var("GIT_CONFIG_GLOBAL");
    env::remove_var("GIT_CONFIG_NOSYSTEM");
}

#[test]
fn int_doctor_verbose_tips_when_desired_off_but_repo_enables_signing() {
    if !git_available() {
        eprintln!("skipping: git not found in PATH");
        return;
    }

    // Transitional self-skip to keep unit lane dockerless and avoid false failures
    if std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
        .ok()
        .as_deref()
        == Some("1")
    {
        eprintln!("skipping: AIFO_CODER_TEST_DISABLE_DOCKER=1");
        return;
    }
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    if !std::process::Command::new("docker")
        .arg("ps")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        eprintln!("skipping: Docker daemon not reachable");
        return;
    }

    // Isolate HOME and global gitconfig
    let td = tempfile::tempdir().expect("tmpdir");
    let home = td.path().to_path_buf();
    env::set_var("HOME", &home);

    let global_cfg = home.join(".gitconfig");
    env::set_var("GIT_CONFIG_GLOBAL", &global_cfg);
    env::set_var("GIT_CONFIG_NOSYSTEM", "1");

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

    let output = run_doctor_capture(true);

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

    // Cleanup env overrides
    env::remove_var("GIT_CONFIG_GLOBAL");
    env::remove_var("GIT_CONFIG_NOSYSTEM");
}
