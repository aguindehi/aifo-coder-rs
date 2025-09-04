use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn have_git() -> bool {
    Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn init_repo(dir: &PathBuf) {
    let _ = Command::new("git").arg("init").current_dir(dir).status();
    let _ = Command::new("git").args(["config","user.name","UT"]).current_dir(dir).status();
    let _ = Command::new("git").args(["config","user.email","ut@example.com"]).current_dir(dir).status();
    let _ = fs::write(dir.join("init.txt"), "x\n");
    let _ = Command::new("git").args(["add","-A"]).current_dir(dir).status();
    let _ = Command::new("git").args(["commit","-m","init"]).current_dir(dir).status();
}

#[test]
fn test_fork_list_json_includes_repo_root_and_default_stale_false() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create a fresh (non-stale) session with created_at ~ now
    let sid = "sid-now";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(
        &Command::new("git")
            .args(["rev-parse","--verify","HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout
    ).trim().to_string();
    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        now_secs, head, pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin)
        .args(["fork","list","--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(out.status.success(), "fork list should succeed");
    let s = String::from_utf8_lossy(&out.stdout);
    // repo_root should be present even in single-repo mode
    assert!(s.contains("\"repo_root\""), "json should include repo_root: {}", s);
    // Default threshold is 14 days -> fresh session should not be stale
    assert!(s.contains("\"stale\":false"), "json should mark stale=false by default: {}", s);
}
