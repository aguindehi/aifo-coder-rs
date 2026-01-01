#![cfg(windows)]
#![allow(clippy::module_name_repetitions)]
// Windows-only helpers extracted from lib.rs for fork orchestration inner builders and wt helpers.

fn ps_quote_inner(s: &str) -> String {
    let esc = s.replace('\'', "''");
    format!("'{}'", esc)
}

/// Build the PowerShell inner command for fork panes (used by tests).
pub fn fork_ps_inner_string(
    agent: &str,
    sid: &str,
    i: usize,
    pane_dir: &std::path::Path,
    pane_state_dir: &std::path::Path,
    child_args: &[String],
) -> String {
    let kv = crate::fork::env::fork_inner_env_kv(agent, sid, i, pane_state_dir);
    let mut assigns: Vec<String> = Vec::new();
    for (k, v) in kv {
        assigns.push(format!("$env:{}={}", k, ps_quote_inner(&v)));
    }
    let cmd = std::env::var("AIFO_CODER_BIN").unwrap_or_else(|_| "aifo-coder".to_string());
    let mut words: Vec<String> = vec![cmd];
    words.extend(child_args.iter().cloned());
    let cmd = words
        .iter()
        .map(|w| ps_quote_inner(w))
        .collect::<Vec<_>>()
        .join(" ");
    let setloc = format!(
        "Set-Location {}",
        ps_quote_inner(&pane_dir.display().to_string())
    );
    format!("{}; {}; {}", setloc, assigns.join("; "), cmd)
}

/// Build the Git Bash inner command for fork panes (used by tests).
pub fn fork_bash_inner_string(
    agent: &str,
    sid: &str,
    i: usize,
    pane_dir: &std::path::Path,
    pane_state_dir: &std::path::Path,
    child_args: &[String],
) -> String {
    let kv = crate::fork::env::fork_inner_env_kv(agent, sid, i, pane_state_dir);
    let mut exports: Vec<String> = Vec::new();
    for (k, v) in kv {
        exports.push(format!("export {}={}", k, crate::shell_escape(&v)));
    }
    let cmd_bin = std::env::var("AIFO_CODER_BIN").unwrap_or_else(|_| "aifo-coder".to_string());
    let mut words: Vec<String> = vec![cmd_bin];
    words.extend(child_args.iter().cloned());
    let cmd = crate::shell_join(&words);
    let cddir = crate::shell_escape(&pane_dir.display().to_string());
    format!("cd {} && {}; {}; exec bash", cddir, exports.join("; "), cmd)
}

/// Map layout to wt.exe split orientation flag.
pub fn wt_orient_for_layout(layout: &str, i: usize) -> &'static str {
    match layout {
        "even-h" => "-H",
        "even-v" => "-V",
        _ => {
            if i % 2 == 0 {
                "-H"
            } else {
                "-V"
            }
        }
    }
}

// Private helper to build the shared tail for wt argument vectors.
fn wt_tail(psbin: &std::path::Path, pane_dir: &std::path::Path, inner: &str) -> Vec<String> {
    vec![
        "-d".to_string(),
        pane_dir.display().to_string(),
        psbin.display().to_string(),
        "-NoExit".to_string(),
        "-Command".to_string(),
        inner.to_string(),
    ]
}

/// Build argument vector for `wt new-tab -d <dir> <psbin> -NoExit -Command <inner>`.
pub fn wt_build_new_tab_args(
    psbin: &std::path::Path,
    pane_dir: &std::path::Path,
    inner: &str,
) -> Vec<String> {
    let mut v = vec!["wt".to_string(), "new-tab".to_string()];
    v.extend(wt_tail(psbin, pane_dir, inner));
    v
}

/// Build argument vector for `wt split-pane <orient> -d <dir> <psbin> -NoExit -Command <inner>`.
pub fn wt_build_split_args(
    orient: &str,
    psbin: &std::path::Path,
    pane_dir: &std::path::Path,
    inner: &str,
) -> Vec<String> {
    let mut v = vec![
        "wt".to_string(),
        "split-pane".to_string(),
        orient.to_string(),
    ];
    v.extend(wt_tail(psbin, pane_dir, inner));
    v
}

/// Build a PowerShell Wait-Process command from a list of PIDs.
pub fn ps_wait_process_cmd(ids: &[&str]) -> String {
    format!("Wait-Process -Id {}", ids.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_helpers_orient_and_builders() {
        let agent = "aider";
        let sid = "sidw";
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pane_dir = tmp.path().join("p");
        std::fs::create_dir_all(&pane_dir).unwrap();
        let state_dir = tmp.path().join("s");
        std::fs::create_dir_all(&state_dir).unwrap();
        let child = vec!["aider".to_string(), "--help".to_string()];

        let ps = fork_ps_inner_string(agent, sid, 1, &pane_dir, &state_dir, &child);
        assert!(
            ps.contains("Set-Location '"),
            "ps inner should set location: {}",
            ps
        );
        assert!(
            ps.contains("$env:AIFO_CODER_SKIP_LOCK='1'"),
            "ps inner should set env"
        );

        let bash = fork_bash_inner_string(agent, sid, 2, &pane_dir, &state_dir, &child);
        assert!(bash.contains("cd "), "bash inner should cd");
        assert!(
            bash.contains("export AIFO_CODER_SKIP_LOCK='1'"),
            "bash inner export env"
        );

        assert_eq!(wt_orient_for_layout("even-h", 3), "-H");
        assert_eq!(wt_orient_for_layout("even-v", 4), "-V");

        let psbin = std::path::PathBuf::from("powershell.exe");
        let inner = "cmds";
        let newtab = wt_build_new_tab_args(&psbin, &pane_dir, inner);
        assert_eq!(newtab[0], "wt");
        assert_eq!(newtab[1], "new-tab");
        let split = wt_build_split_args("-H", &psbin, &pane_dir, inner);
        assert_eq!(split[1], "split-pane");
        assert_eq!(split[2], "-H");

        let w = crate::ps_wait_process_cmd(&["101", "202", "303"]);
        assert_eq!(w, "Wait-Process -Id 101,202,303");
    }
}
