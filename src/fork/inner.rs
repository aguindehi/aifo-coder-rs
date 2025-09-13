use super::types::{ForkSession, Pane};

#[cfg(windows)]
/// Build a PowerShell inner command string using the library helper,
/// then inject AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 immediately after Set-Location.
pub fn build_inner_powershell(session: &ForkSession, pane: &Pane, child_args: &[String]) -> String {
    let s = aifo_coder::fork_ps_inner_string(
        &session.agent,
        &session.sid,
        pane.index,
        &pane.dir,
        &pane.state_dir,
        child_args,
    );
    // Insert "$env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1';" after the first "; "
    if let Some(pos) = s.find("; ") {
        let (head, tail) = s.split_at(pos + 2);
        format!(
            "{}$env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1'; {}",
            head, tail
        )
    } else {
        // Fallback: append at start of assignments
        format!("{}; $env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1';", s)
    }
}

#[cfg(not(windows))]
pub fn build_inner_powershell(_session: &ForkSession, _pane: &Pane, _child_args: &[String]) -> String {
    String::new()
}

#[cfg(windows)]
#[test]
fn test_gitbash_inner_keeps_exec_tail_when_no_post_merge() {
    use std::path::PathBuf;
    let session = ForkSession {
        sid: "sid123".to_string(),
        session_name: "sess".to_string(),
        base_label: "main".to_string(),
        base_ref_or_sha: "main".to_string(),
        base_commit_sha: "deadbeef".to_string(),
        created_at: 0,
        layout: "tiled".to_string(),
        agent: "aider".to_string(),
        session_dir: PathBuf::from("."),
    };
    let pane = Pane {
        index: 1,
        dir: PathBuf::from("."),
        branch: "branch".to_string(),
        state_dir: PathBuf::from("./state"),
        container_name: "aifo-coder-aider-sid123-1".to_string(),
    };
    let child = vec!["aider".to_string(), "--help".to_string()];
    // exec_shell_tail=true -> keep "; exec bash"
    let s = build_inner_gitbash(&session, &pane, &child, true);
    assert!(
        s.contains("export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; aifo-coder"),
        "Git Bash inner should inject SUPPRESS var before aifo-coder, got: {}",
        s
    );
    assert!(
        s.ends_with("; exec bash"),
        "Git Bash inner should keep '; exec bash' when exec_shell_tail=true, got: {}",
        s
    );
}

#[cfg(windows)]
/// Build a Git Bash inner command string using the library helper,
/// then inject export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; before the agent command.
/// When exec_shell_tail=false, trim the trailing "; exec bash" from the inner string.
pub fn build_inner_gitbash(
    session: &ForkSession,
    pane: &Pane,
    child_args: &[String],
    exec_shell_tail: bool,
) -> String {
    let mut s = aifo_coder::fork_bash_inner_string(
        &session.agent,
        &session.sid,
        pane.index,
        &pane.dir,
        &pane.state_dir,
        child_args,
    );
    // Inject SUPPRESS just before "aifo-coder" invocation.
    // Replace "; aifo-coder" with "; export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; aifo-coder"
    let needle = "; aifo-coder";
    if let Some(pos) = s.find(needle) {
        let mut out = String::with_capacity(s.len() + 50);
        out.push_str(&s[..pos]);
        out.push_str("; export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1");
        out.push_str(needle);
        out.push_str(&s[pos + needle.len()..]);
        s = out;
    }
    if !exec_shell_tail && s.ends_with("; exec bash") {
        let cut = s.len() - "; exec bash".len();
        s.truncate(cut);
    }
    s
}

#[cfg(not(windows))]
pub fn build_inner_gitbash(
    _session: &ForkSession,
    _pane: &Pane,
    _child_args: &[String],
    _exec_shell_tail: bool,
) -> String {
    String::new()
}

/// Build the tmux launch script content with the same "press 's' to open a shell" logic.
pub fn build_tmux_launch_script(
    session: &ForkSession,
    pane: &Pane,
    child_args_joined: &str,
    _launcher_path: &str,
) -> String {
    let mut exports: Vec<String> = Vec::new();
    for (k, v) in [
        (
            "AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING".to_string(),
            "1".to_string(),
        ),
        ("AIFO_CODER_SKIP_LOCK".to_string(), "1".to_string()),
        (
            "AIFO_CODER_CONTAINER_NAME".to_string(),
            pane.container_name.clone(),
        ),
        (
            "AIFO_CODER_HOSTNAME".to_string(),
            pane.container_name.clone(),
        ),
        ("AIFO_CODER_FORK_SESSION".to_string(), session.sid.clone()),
        ("AIFO_CODER_FORK_INDEX".to_string(), pane.index.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR".to_string(),
            pane.state_dir.display().to_string(),
        ),
    ] {
        exports.push(format!("export {}={}", k, aifo_coder::shell_escape(&v)));
    }
    format!(
        r#"#!/usr/bin/env bash
set -e
{}
set +e
{}
st=$?
if [ -t 0 ] && command -v tmux >/dev/null 2>&1; then
  pid="$(tmux display -p '#{{pane_id}}')"
  secs="${{AIFO_CODER_FORK_SHELL_PROMPT_SECS:-5}}"
  printf "aifo-coder: agent exited (code %s). Press 's' to open a shell, or wait: " "$st"
  for ((i=secs; i>=1; i--)); do
    printf "%s " "$i"
    if IFS= read -rsn1 -t 1 ch; then
      echo
      if [[ -z "$ch" || "$ch" == $'\n' || "$ch" == $'\r' ]]; then
        tmux kill-pane -t "$pid" >/dev/null 2>&1 || exit "$st"
        exit "$st"
      elif [[ "$ch" == 's' || "$ch" == 'S' ]]; then
        exec "${{SHELL:-sh}}"
      fi
    fi
  done
  echo
  tmux kill-pane -t "$pid" >/dev/null 2>&1 || exit "$st"
else
  exit "$st"
fi
"#,
        exports.join("\n"),
        child_args_joined
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_session_and_pane() -> (ForkSession, Pane) {
        let session = ForkSession {
            sid: "sid123".to_string(),
            session_name: "sess".to_string(),
            base_label: "main".to_string(),
            base_ref_or_sha: "main".to_string(),
            base_commit_sha: "deadbeef".to_string(),
            created_at: 0,
            layout: "tiled".to_string(),
            agent: "aider".to_string(),
            session_dir: PathBuf::from("."),
        };
        let pane = Pane {
            index: 1,
            dir: PathBuf::from("."),
            branch: "branch".to_string(),
            state_dir: PathBuf::from("./state"),
            container_name: "aifo-coder-aider-sid123-1".to_string(),
        };
        (session, pane)
    }

    #[cfg(windows)]
    #[test]
    fn test_ps_inner_injects_suppress_env() {
        let (session, pane) = make_session_and_pane();
        let child = vec!["aider".to_string(), "--help".to_string()];
        let s = build_inner_powershell(&session, &pane, &child);
        assert!(
            s.contains("$env:AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING='1';"),
            "PowerShell inner should inject SUPPRESS var, got: {}",
            s
        );
        assert!(
            s.contains("Set-Location "),
            "PowerShell inner should include Set-Location prefix"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_gitbash_inner_injects_and_trims_exec_tail_when_post_merge_requested() {
        let (session, pane) = make_session_and_pane();
        let child = vec!["aider".to_string(), "--help".to_string()];
        // exec_shell_tail=false simulates post-merge requested (should trim '; exec bash')
        let s = build_inner_gitbash(&session, &pane, &child, false);
        assert!(
            s.contains("export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1; aifo-coder"),
            "Git Bash inner should inject SUPPRESS var before aifo-coder, got: {}",
            s
        );
        assert!(
            !s.ends_with("; exec bash"),
            "Git Bash inner should trim '; exec bash' when exec_shell_tail=false, got: {}",
            s
        );
    }

    #[test]
    fn test_tmux_launch_script_contains_expected_exports_and_prompt() {
        let (session, mut pane) = make_session_and_pane();
        // Ensure container name and state_dir are set deterministically
        pane.container_name = "aifo-coder-aider-sid123-1".to_string();
        pane.state_dir = PathBuf::from("./state");
        let script = build_tmux_launch_script(&session, &pane, "echo hi", "/launcher");
        // Must contain env exports
        assert!(
            script.contains("export AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1"),
            "tmux script should export SUPPRESS var: {}",
            script
        );
        assert!(
            script.contains("export AIFO_CODER_FORK_SESSION=sid123"),
            "tmux script should export fork session id: {}",
            script
        );
        assert!(
            script.contains("Press 's' to open a shell"),
            "tmux script should include press 's' prompt: {}",
            script
        );
        assert!(
            script.contains("echo hi"),
            "tmux script should include child args payload: {}",
            script
        );
    }
}
