use std::fs;
use std::path::PathBuf;
mod support;
use support::{have_git, init_repo_with_default_user};

// Helper: initialize a minimal git repo with one commit
fn init_repo(dir: &PathBuf) {
    let _ = init_repo_with_default_user(dir.as_path());
}

#[test]
fn int_test_fork_clean_refuses_when_protected_without_overrides() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    // Set up a base git repo to act as current repo
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create a fork session with one pane that is dirty
    let sid = "sid1";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane1 = base.join("pane-1");
    fs::create_dir_all(&pane1).unwrap();
    init_repo(&pane1);

    // Write .meta.json with base_commit_sha matching current HEAD
    let head = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(&pane1)
        .output()
        .unwrap();
    let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_sha,
        pane1.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    // Make the pane dirty
    fs::write(pane1.join("dirty.txt"), "d\n").unwrap();

    // Run fork_clean without overrides: should refuse (return code 1)
    let opts = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: false,
        force: false,
        keep_dirty: false,
        json: false,
    };
    let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean");
    assert_eq!(code, 1, "expected refusal when pane is protected");
    assert!(base.exists(), "session dir must remain");
}

#[test]
fn int_test_fork_clean_keep_dirty_removes_only_clean_panes_and_updates_meta() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let sid = "sid2";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane_clean = base.join("pane-1");
    let pane_dirty = base.join("pane-2");
    fs::create_dir_all(&pane_clean).unwrap();
    fs::create_dir_all(&pane_dirty).unwrap();
    init_repo(&pane_clean);
    init_repo(&pane_dirty);

    // HEAD sha
    let head_clean = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_clean)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();

    // Make pane-2 dirty
    fs::write(pane_dirty.join("dirty.txt"), "d\n").unwrap();

    // Meta with both panes; base_commit set to head_clean (both at same HEAD initially)
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 2, \"pane_dirs\": [\"{}\",\"{}\"], \"branches\": [\"fork/main/{sid}-1\",\"fork/main/{sid}-2\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        head_clean,
        pane_clean.display(),
        pane_dirty.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let opts = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: true, // skip prompt in CI
        force: false,
        keep_dirty: true,
        json: false,
    };
    let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean keep-dirty");
    assert_eq!(code, 0, "keep-dirty should succeed");
    assert!(!pane_clean.exists(), "clean pane should be deleted");
    assert!(pane_dirty.exists(), "dirty pane should remain");
    // Meta should still exist and list remaining pane-2 directory
    let meta2 = fs::read_to_string(base.join(".meta.json")).expect("read meta");
    assert!(
        meta2.contains("panes_remaining"),
        "meta should contain panes_remaining"
    );
    assert!(
        meta2.contains(&format!("\"{}\"", pane_dirty.display())),
        "meta should include remaining pane dir"
    );
}

#[test]
fn int_test_fork_autoclean_deletes_old_clean_session() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create one clean session older than threshold
    let sid = "sid-old";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let old_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 40 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs,
        head,
        pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    // Ensure env enables autoclean with threshold 30 days
    std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", "1");
    std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", "30");

    // Change CWD into repo so repo_root() can find it
    let old_cwd = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();
    aifo_coder::fork_autoclean_if_enabled();
    std::env::set_current_dir(old_cwd).unwrap();

    assert!(
        !base.exists(),
        "old clean session should have been auto-removed"
    );
}

#[test]
fn int_test_fork_list_json_stale_flag() {
    // Skip if git binary is missing
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create a stale session (created_at older than threshold)
    let sid = "sid-stale";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let old_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 2 * 86400;
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        old_secs, head, pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    // Run CLI and assert stale=true in JSON
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = std::process::Command::new(bin)
        .args(["fork", "list", "--json"])
        .env("AIFO_CODER_FORK_LIST_STALE_DAYS", "1")
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(out.status.success(), "fork list should succeed");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("\"sid\":\"sid-stale\""),
        "json must include sid-stale: {}",
        s
    );
    assert!(
        s.contains("\"stale\":true"),
        "json must mark stale=true: {}",
        s
    );
}

#[test]
fn int_test_fork_list_all_repos_json_and_env_requirement() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let ws = tempfile::tempdir().expect("tmpdir");
    let ws_path = ws.path().to_path_buf();

    // repo A
    let repo_a = ws_path.join("repo-a");
    fs::create_dir_all(&repo_a).unwrap();
    init_repo(&repo_a);
    let sid_a = "sid-a";
    let base_a = repo_a.join(".aifo-coder").join("forks").join(sid_a);
    let pane_a = base_a.join("pane-1");
    fs::create_dir_all(&pane_a).unwrap();
    init_repo(&pane_a);
    let head_a = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_a)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let meta_a = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid_a}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), head_a, pane_a.display()
    );
    fs::create_dir_all(&base_a).unwrap();
    fs::write(base_a.join(".meta.json"), meta_a).unwrap();

    // repo B
    let repo_b = ws_path.join("repo-b");
    fs::create_dir_all(&repo_b).unwrap();
    init_repo(&repo_b);
    let sid_b = "sid-b";
    let base_b = repo_b.join(".aifo-coder").join("forks").join(sid_b);
    let pane_b = base_b.join("pane-1");
    fs::create_dir_all(&pane_b).unwrap();
    init_repo(&pane_b);
    let head_b = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane_b)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let meta_b = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid_b}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), head_b, pane_b.display()
    );
    fs::create_dir_all(&base_b).unwrap();
    fs::write(base_b.join(".meta.json"), meta_b).unwrap();

    // Run CLI with --all-repos and WORKSPACE root set
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = std::process::Command::new(bin)
        .args(["fork", "list", "--json", "--all-repos"])
        .env("AIFO_CODER_WORKSPACE_ROOT", &ws_path)
        .current_dir(&ws_path)
        .output()
        .expect("run aifo-coder fork list --json --all-repos");
    assert!(out.status.success(), "fork list --all-repos should succeed");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("\"sid\":\"sid-a\""),
        "json must include sid-a: {}",
        s
    );
    assert!(
        s.contains("\"sid\":\"sid-b\""),
        "json must include sid-b: {}",
        s
    );

    // Now run without workspace env and expect failure
    let out2 = std::process::Command::new(bin)
        .args(["fork", "list", "--all-repos"])
        .current_dir(&ws_path)
        .output()
        .expect("run aifo-coder fork list --all-repos");
    assert!(
        !out2.status.success(),
        "fork list --all-repos without env should fail"
    );
}

#[test]
fn int_test_fork_list_json_empty_when_no_sessions() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = std::process::Command::new(bin)
        .args(["fork", "list", "--json"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork list --json");
    assert!(out.status.success(), "fork list should succeed");
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert_eq!(
        s, "[]",
        "expected empty JSON array when no sessions, got: {}",
        s
    );
}

#[test]
fn int_test_fork_clean_json_plan_and_exec() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create one clean session
    let sid = "sid-plan";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);
    let head = String::from_utf8_lossy(
        &std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap()
            .stdout,
    )
    .trim()
    .to_string();
    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(), head, pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    // Dry-run plan JSON
    let out = std::process::Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--json", "--dry-run"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork clean --json --dry-run");
    assert!(out.status.success(), "clean dry-run should succeed");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("\"plan\":true"),
        "plan JSON must include plan=true: {}",
        s
    );
    assert!(
        s.contains("\"sid\":\"sid-plan\"") || s.contains("\"sid\":\"sid-plan\""),
        "plan JSON should include sid-plan: {}",
        s
    );

    // Execute JSON
    let out2 = std::process::Command::new(bin)
        .args(["fork", "clean", "--session", sid, "--json", "--yes"])
        .current_dir(&root)
        .output()
        .expect("run aifo-coder fork clean --json --yes");
    assert!(out2.status.success(), "clean exec should succeed");
    let s2 = String::from_utf8_lossy(&out2.stdout);
    assert!(
        s2.contains("\"deleted_sessions\":1"),
        "result JSON should report deleted_sessions=1: {}",
        s2
    );
    assert!(!base.exists(), "session directory should be deleted");
}

#[test]
fn int_test_fork_clean_base_unknown_is_protected() {
    if !have_git() {
        eprintln!("skipping: git not found in PATH");
        return;
    }
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path().to_path_buf();
    init_repo(&root);

    // Create a session with unresolvable base_commit_sha to trigger base-unknown
    let sid = "sid-unknown";
    let base = root.join(".aifo-coder").join("forks").join(sid);
    let pane = base.join("pane-1");
    fs::create_dir_all(&pane).unwrap();
    init_repo(&pane);

    let meta = format!(
        "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"deadbeefdeadbeefdeadbeefdeadbeefdeadbeef\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        pane.display()
    );
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join(".meta.json"), meta).unwrap();

    let opts = aifo_coder::ForkCleanOpts {
        session: Some(sid.to_string()),
        older_than_days: None,
        all: false,
        dry_run: false,
        yes: false,
        force: false,
        keep_dirty: false,
        json: false,
    };
    let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean");
    assert_eq!(code, 1, "expected refusal when base commit is unknown");
    assert!(base.exists(), "session dir must remain");
}
