#[cfg(windows)]
use which::which;

use crate::cli::Cli;
use super::types::{ForkSession, Pane};

pub trait Orchestrator {
    fn launch(&self, session: &ForkSession, panes: &[Pane], child_args: &[String]) -> Result<(), String>;
    fn supports_post_merge(&self) -> bool;
}

pub enum Selected {
    #[cfg(not(windows))]
    Tmux { reason: String },
    #[cfg(windows)]
    WindowsTerminal { reason: String },
    #[cfg(windows)]
    PowerShell { reason: String },
    #[cfg(windows)]
    GitBashMintty { reason: String },
}

#[cfg(windows)]
fn have(bin: &str) -> bool {
    #[cfg(test)]
    {
        // Allow tests to simulate tool presence via env flags:
        // e.g., AIFO_TEST_HAVE_WT=1, AIFO_TEST_HAVE_PWSH=1, AIFO_TEST_HAVE_GIT_BASH_EXE=1, AIFO_TEST_HAVE_MINTTY_EXE=1
        let key = format!(
            "AIFO_TEST_HAVE_{}",
            bin.replace('.', "_").replace('-', "_").to_ascii_uppercase()
        );
        if let Ok(v) = std::env::var(&key) {
            if v.trim() == "1" {
                return true;
            }
            if v.trim() == "0" {
                return false;
            }
        }
    }
    which(bin).is_ok()
}

#[cfg(windows)]
fn have_any<I: IntoIterator<Item = &'static str>>(bins: I) -> bool {
    bins.into_iter().any(|b| have(b))
}

/// Select orchestrator with corrected rules from spec (Windows env precedence).
pub fn select_orchestrator(cli: &Cli, _layout_requested: &str) -> Selected {
    #[cfg(not(windows))]
    {
        Selected::Tmux { reason: "non-Windows host, using tmux".to_string() }
    }

    #[cfg(windows)]
    {
        let pref = std::env::var("AIFO_CODER_FORK_ORCH").ok().unwrap_or_default().to_ascii_lowercase();

        if pref.as_str() == "gitbash" {
            if have_any(["git-bash.exe", "bash.exe"]) || have("mintty.exe") {
                return Selected::GitBashMintty { reason: "AIFO_CODER_FORK_ORCH=gitbash".to_string() };
            } else {
                // Caller should emit the exact error text; here we still return fallback selection to avoid panics.
                return Selected::GitBashMintty { reason: "requested gitbash but not found".to_string() };
            }
        }
        if pref.as_str() == "powershell" {
            if have_any(["pwsh", "powershell", "powershell.exe"]) {
                return Selected::PowerShell { reason: "AIFO_CODER_FORK_ORCH=powershell".to_string() };
            }
            // fallthrough to wt selection below
        }

        if have_any(["wt", "wt.exe"]) {
            if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                // Fallback to PowerShell to support waiting
                if have_any(["pwsh", "powershell", "powershell.exe"]) {
                    return Selected::PowerShell { reason: "wt present but merge requested; using PowerShell".to_string() };
                }
                // otherwise keep Windows Terminal (non-waitable) and higher-level prints guidance
            }
            return Selected::WindowsTerminal { reason: "wt present".to_string() };
        }

        if have_any(["pwsh", "powershell", "powershell.exe"]) {
            return Selected::PowerShell { reason: "PowerShell present".to_string() };
        }
        if have_any(["git-bash.exe", "bash.exe", "mintty.exe"]) {
            return Selected::GitBashMintty { reason: "Git Bash/mintty present".to_string() };
        }
        // Final fallback — upstream caller prints the exact error message and exits 127.
        Selected::WindowsTerminal { reason: "none found".to_string() }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use clap::Parser;

    fn reset_env() {
        for k in [
            "AIFO_CODER_FORK_ORCH",
            "AIFO_TEST_HAVE_WT",
            "AIFO_TEST_HAVE_WT_EXE",
            "AIFO_TEST_HAVE_PWSH",
            "AIFO_TEST_HAVE_POWERSHELL",
            "AIFO_TEST_HAVE_POWERSHELL_EXE",
            "AIFO_TEST_HAVE_GIT_BASH_EXE",
            "AIFO_TEST_HAVE_BASH_EXE",
            "AIFO_TEST_HAVE_MINTTY_EXE",
        ] {
            std::env::remove_var(k);
        }
    }

    fn make_cli(args: &[&str]) -> crate::cli::Cli {
        // Minimal parser invocation; command defaults to aider when unspecified
        let mut v = vec!["aifo-coder"];
        v.extend(args);
        crate::cli::Cli::parse_from(v)
    }

    #[test]
    fn test_select_orchestrator_pref_powershell_when_available() {
        reset_env();
        std::env::set_var("AIFO_CODER_FORK_ORCH", "powershell");
        std::env::set_var("AIFO_TEST_HAVE_PWSH", "1");
        let cli = make_cli(&["aider"]);
        let sel = select_orchestrator(&cli, "tiled");
        match sel {
            Selected::PowerShell { .. } => {}
            other => panic!("expected PowerShell selection, got {:?}", std::mem::discriminant(&other)),
        }
        reset_env();
    }

    #[test]
    fn test_select_orchestrator_pref_gitbash_when_available() {
        reset_env();
        std::env::set_var("AIFO_CODER_FORK_ORCH", "gitbash");
        std::env::set_var("AIFO_TEST_HAVE_GIT_BASH_EXE", "1");
        let cli = make_cli(&["aider"]);
        let sel = select_orchestrator(&cli, "tiled");
        match sel {
            Selected::GitBashMintty { .. } => {}
            other => panic!("expected Git Bash selection, got {:?}", std::mem::discriminant(&other)),
        }
        reset_env();
    }

    #[test]
    fn test_select_orchestrator_prefers_wt_when_no_merge_requested() {
        reset_env();
        std::env::set_var("AIFO_TEST_HAVE_WT", "1");
        let cli = make_cli(&["aider"]);
        let sel = select_orchestrator(&cli, "tiled");
        match sel {
            Selected::WindowsTerminal { .. } => {}
            other => panic!("expected Windows Terminal selection, got {:?}", std::mem::discriminant(&other)),
        }
        reset_env();
    }

    #[test]
    fn test_select_orchestrator_falls_back_to_powershell_when_merge_requested() {
        reset_env();
        // wt present, merge requested, and pwsh available -> PowerShell fallback
        std::env::set_var("AIFO_TEST_HAVE_WT", "1");
        std::env::set_var("AIFO_TEST_HAVE_PWSH", "1");
        let cli = make_cli(&["--fork-merge-strategy", "fetch", "aider"]);
        let sel = select_orchestrator(&cli, "tiled");
        match sel {
            Selected::PowerShell { .. } => {}
            other => panic!("expected PowerShell selection for merge, got {:?}", std::mem::discriminant(&other)),
        }
        reset_env();
    }

    #[test]
    fn test_select_orchestrator_none_found_path() {
        reset_env();
        // Explicitly mark all as absent
        for (k, v) in [
            ("AIFO_TEST_HAVE_WT", "0"),
            ("AIFO_TEST_HAVE_WT_EXE", "0"),
            ("AIFO_TEST_HAVE_PWSH", "0"),
            ("AIFO_TEST_HAVE_POWERSHELL", "0"),
            ("AIFO_TEST_HAVE_POWERSHELL_EXE", "0"),
            ("AIFO_TEST_HAVE_GIT_BASH_EXE", "0"),
            ("AIFO_TEST_HAVE_BASH_EXE", "0"),
            ("AIFO_TEST_HAVE_MINTTY_EXE", "0"),
        ] {
            std::env::set_var(k, v);
        }
        let cli = make_cli(&["aider"]);
        let sel = select_orchestrator(&cli, "tiled");
        match sel {
            Selected::WindowsTerminal { .. } => {} // "none found" sentinel variant
            other => panic!("expected WindowsTerminal 'none found' selection, got {:?}", std::mem::discriminant(&other)),
        }
        reset_env();
    }

    #[test]
    fn test_select_orchestrator_wt_merge_requested_without_pwsh_stays_wt() {
        reset_env();
        // WT present, merge requested, but no PowerShell available -> keep WT (non-waitable)
        std::env::set_var("AIFO_TEST_HAVE_WT", "1");
        std::env::set_var("AIFO_TEST_HAVE_WT_EXE", "1");
        // Ensure PowerShell env flags are absent/false
        for (k, v) in [
            ("AIFO_TEST_HAVE_PWSH", "0"),
            ("AIFO_TEST_HAVE_POWERSHELL", "0"),
            ("AIFO_TEST_HAVE_POWERSHELL_EXE", "0"),
        ] {
            std::env::set_var(k, v);
        }
        let cli = make_cli(&["--fork-merge-strategy", "fetch", "aider"]);
        let sel = select_orchestrator(&cli, "tiled");
        match sel {
            Selected::WindowsTerminal { .. } => {}
            other => panic!("expected WindowsTerminal selection when pwsh unavailable, got {:?}", std::mem::discriminant(&other)),
        }
        reset_env();
    }
}
