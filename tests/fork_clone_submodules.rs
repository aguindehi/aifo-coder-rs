use std::process::Command;
mod support;
use support::have_git;

#[test]
fn test_fork_clone_and_checkout_panes_inits_submodules() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");

    // Create submodule repository
    let sub = td.path().join("sm");
    std::fs::create_dir_all(&sub).expect("mkdir sm");
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(&sub)
        .status()
        .unwrap()
        .success());
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(&sub)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(&sub)
        .status();
    std::fs::write(sub.join("sub.txt"), "sub\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&sub)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "sub init"])
        .current_dir(&sub)
        .status()
        .unwrap()
        .success());

    // Create base repository and add submodule
    let base = td.path().join("base");
    std::fs::create_dir_all(&base).expect("mkdir base");
    assert!(Command::new("git")
        .args(["init"])
        .current_dir(&base)
        .status()
        .unwrap()
        .success());
    let _ = Command::new("git")
        .args(["config", "user.name", "AIFO Test"])
        .current_dir(&base)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.email", "aifo@example.com"])
        .current_dir(&base)
        .status();
    std::fs::write(base.join("file.txt"), "x\n").unwrap();
    assert!(Command::new("git")
        .args(["add", "-A"])
        .current_dir(&base)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "base init"])
        .current_dir(&base)
        .status()
        .unwrap()
        .success());

    let sub_path = sub.display().to_string();
    assert!(Command::new("git")
        .args([
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            &sub_path,
            "submod"
        ])
        .current_dir(&base)
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args(["commit", "-m", "add submodule"])
        .current_dir(&base)
        .status()
        .unwrap()
        .success());

    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&base)
        .output()
        .unwrap();
    let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let base_label = aifo_coder::fork_sanitize_base_label(&cur_branch);

    let res = aifo_coder::fork_clone_and_checkout_panes(
        &base,
        "sid-sub",
        1,
        &cur_branch,
        &base_label,
        false,
    )
    .expect("clone panes with submodule");
    assert_eq!(res.len(), 1);
    let pane_dir = &res[0].0;
    let sub_file = pane_dir.join("submod").join("sub.txt");
    assert!(
        sub_file.exists(),
        "expected submodule file to exist in clone: {}",
        sub_file.display()
    );
}
