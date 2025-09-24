use std::process::ExitCode;

/// Ensure git and required orchestrators are present on this platform, printing exact messages.
/// Returns Err(ExitCode) on failure paths using the same codes as in main.rs.
pub fn ensure_git_and_orchestrator_present_on_platform() -> Result<(), ExitCode> {
    if which::which("git").is_err() {
        eprintln!("aifo-coder: error: git is required and was not found in PATH.");
        return Err(ExitCode::from(1));
    }
    if cfg!(target_os = "windows") {
        let wt_ok = which::which("wt")
            .or_else(|_| which::which("wt.exe"))
            .is_ok();
        let ps_ok = which::which("pwsh")
            .or_else(|_| which::which("powershell"))
            .or_else(|_| which::which("powershell.exe"))
            .is_ok();
        let gb_ok = which::which("git-bash.exe")
            .or_else(|_| which::which("bash.exe"))
            .or_else(|_| which::which("mintty.exe"))
            .is_ok();
        if !(wt_ok || ps_ok || gb_ok) {
            eprintln!("aifo-coder: error: none of Windows Terminal (wt.exe), PowerShell, or Git Bash were found in PATH.");
            return Err(ExitCode::from(127));
        }
    } else if which::which("tmux").is_err() {
        eprintln!("aifo-coder: error: tmux not found. Please install tmux to use fork mode.");
        return Err(ExitCode::from(127));
    }
    Ok(())
}

/// Guard when launching many panes and prompt for confirmation (same message as main.rs).
pub fn guard_panes_count_and_prompt(panes: usize) -> Result<(), ExitCode> {
    if panes > 8 {
        let msg = format!(
            "Launching {} panes may impact disk/memory and I/O performance.",
            panes
        );
        if !crate::warn_prompt_continue_or_quit(&[&msg]) {
            return Err(ExitCode::from(1));
        }
    }
    Ok(())
}

/// Guard against zero panes, printing the exact message and returning ExitCode 1.
#[allow(dead_code)]
pub fn guard_no_panes(clones_len: usize) -> Result<(), ExitCode> {
    if clones_len == 0 {
        eprintln!("aifo-coder: no panes to create.");
        return Err(ExitCode::from(1));
    }
    Ok(())
}
