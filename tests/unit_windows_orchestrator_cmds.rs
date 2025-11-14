#![cfg(windows)]

use std::path::PathBuf;

#[test]
fn test_ps_inner_contains_set_location_and_env() {
    let agent = "aider";
    let sid = "sid123";
    let i = 2usize;
    let pane_dir = PathBuf::from(r"C:\Users\Test User\project\fork\pane-2");
    let pane_state_dir = PathBuf::from(r"C:\State Base\sid123\pane-2");
    let child_args = vec!["aider".to_string(), "--help".to_string()];

    let inner =
        aifo_coder::fork_ps_inner_string(agent, sid, i, &pane_dir, &pane_state_dir, &child_args);

    let setloc = format!("Set-Location '{}'", pane_dir.display());
    assert!(
        inner.contains(&setloc),
        "PowerShell inner missing Set-Location: {}",
        inner
    );
    let fsd = format!(
        "$env:AIFO_CODER_FORK_STATE_DIR='{}'",
        pane_state_dir.display()
    );
    assert!(
        inner.contains(&fsd),
        "PowerShell inner missing FORK_STATE_DIR env: {}",
        inner
    );
    assert!(
        inner.contains("$env:AIFO_CODER_SKIP_LOCK='1'"),
        "PowerShell inner missing SKIP_LOCK env: {}",
        inner
    );
    // Child command quoting
    assert!(
        inner.contains("'aifo-coder' 'aider' '--help'"),
        "PowerShell inner missing child command: {}",
        inner
    );
}

#[test]
fn test_bash_inner_format_and_exports() {
    let agent = "aider";
    let sid = "sidX";
    let i = 1usize;
    let pane_dir = PathBuf::from(r"C:\Users\Foo Bar\repo\fork\pane-1");
    let pane_state_dir = PathBuf::from(r"C:\State Dir\sidX\pane-1");
    let child_args = vec!["aider".to_string(), "--version".to_string()];

    let inner =
        aifo_coder::fork_bash_inner_string(agent, sid, i, &pane_dir, &pane_state_dir, &child_args);

    let cddir = format!("cd '{}'", pane_dir.display());
    assert!(
        inner.starts_with(&cddir),
        "Bash inner must start with cd: {}",
        inner
    );
    assert!(
        inner.contains("export AIFO_CODER_SKIP_LOCK='1'"),
        "Bash inner missing SKIP_LOCK export: {}",
        inner
    );
    assert!(
        inner.contains("export AIFO_CODER_FORK_STATE_DIR="),
        "Bash inner missing FORK_STATE_DIR export: {}",
        inner
    );
    assert!(
        inner.contains("aifo-coder"),
        "Bash inner missing aifo-coder child invocation: {}",
        inner
    );
    assert!(
        inner.ends_with("exec bash"),
        "Bash inner should end with 'exec bash': {}",
        inner
    );
}

#[test]
fn test_wt_orient_for_layout_mapping() {
    // even-h -> -H, even-v -> -V
    assert_eq!(aifo_coder::wt_orient_for_layout("even-h", 1), "-H");
    assert_eq!(aifo_coder::wt_orient_for_layout("even-v", 1), "-V");
    // tiled alternates: odd -> -V, even -> -H (as implemented)
    assert_eq!(aifo_coder::wt_orient_for_layout("tiled", 1), "-V");
    assert_eq!(aifo_coder::wt_orient_for_layout("tiled", 2), "-H");
}

#[test]
fn test_wt_build_new_tab_args_contains_d_flag() {
    let ps = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let dir = PathBuf::from(r"C:\Work\repo\fork\pane-1");
    let inner = "echo hi";
    let args = aifo_coder::wt_build_new_tab_args(&ps, &dir, inner);
    assert_eq!(args.get(0), Some(&"wt".to_string()));
    assert_eq!(args.get(1), Some(&"new-tab".to_string()));
    let pos = args
        .iter()
        .position(|s| s == "-d")
        .expect("missing -d flag");
    assert_eq!(args.get(pos + 1), Some(&dir.display().to_string()));
    // Ensure powershell path and -Command are present
    assert!(
        args.contains(&ps.display().to_string()),
        "psbin path missing in args: {:?}",
        args
    );
    assert!(
        args.contains(&"-Command".to_string()),
        "-Command missing in args: {:?}",
        args
    );
}

#[test]
fn test_wt_build_split_args_contains_orient_and_d() {
    let ps = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let dir = PathBuf::from(r"C:\Work\repo\fork\pane-2");
    let inner = "echo bye";
    let args = aifo_coder::wt_build_split_args("-H", &ps, &dir, inner);
    assert_eq!(args.get(0), Some(&"wt".to_string()));
    assert_eq!(args.get(1), Some(&"split-pane".to_string()));
    assert!(
        args.contains(&"-H".to_string()),
        "orientation flag missing: {:?}",
        args
    );
    let pos = args
        .iter()
        .position(|s| s == "-d")
        .expect("missing -d flag");
    assert_eq!(args.get(pos + 1), Some(&dir.display().to_string()));
    assert!(
        args.contains(&ps.display().to_string()),
        "psbin path missing in args: {:?}",
        args
    );
    assert!(
        args.contains(&"-Command".to_string()),
        "-Command missing in args: {:?}",
        args
    );
}
