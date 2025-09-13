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
    which(bin).is_ok()
}

#[cfg(windows)]
fn have_any<I: IntoIterator<Item = &'static str>>(bins: I) -> bool {
    bins.into_iter().any(|b| have(b))
}

/// Select orchestrator with corrected rules from spec (Windows env precedence).
pub fn select_orchestrator(cli: &Cli, layout_requested: &str) -> Selected {
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
