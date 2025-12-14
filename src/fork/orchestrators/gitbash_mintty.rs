#![cfg(windows)]

use std::process::Command;

use super::super::types::{ForkSession, Pane};
use super::Orchestrator;
use crate::reject_newlines;

/// Git Bash / mintty orchestrator.
/// When exec_shell_tail is false, trims the trailing "; exec bash" from inner.
pub struct GitBashMintty {
    pub exec_shell_tail: bool,
}

impl Orchestrator for GitBashMintty {
    fn launch(
        &self,
        session: &ForkSession,
        panes: &[Pane],
        child_args: &[String],
    ) -> Result<(), String> {
        // Prefer git-bash.exe or bash.exe; otherwise try mintty.exe -e bash -lc ...
        let git_bash = which::which("git-bash.exe").or_else(|_| which::which("bash.exe"));
        if let Ok(gb) = git_bash {
            for p in panes {
                let mut inner = aifo_coder::fork_bash_inner_string(
                    &session.agent,
                    &session.sid,
                    p.index,
                    &p.dir,
                    &p.state_dir,
                    child_args,
                );
                if !self.exec_shell_tail && inner.ends_with("; exec bash") {
                    let cut = inner.len() - "; exec bash".len();
                    inner.truncate(cut);
                }
                reject_newlines(&inner, "Git Bash inner command")?;
                let st = Command::new(&gb).arg("-c").arg(&inner).status();
                match st {
                    Ok(s) if s.success() => {}
                    _ => return Err("failed to launch a Git Bash window".to_string()),
                }
            }
            return Ok(());
        }

        // Fallback: mintty.exe
        let mt = which::which("mintty.exe").map_err(|_| {
            "neither Git Bash (git-bash.exe/bash.exe) nor mintty.exe found in PATH".to_string()
        })?;
        for p in panes {
            let mut inner = aifo_coder::fork_bash_inner_string(
                &session.agent,
                &session.sid,
                p.index,
                &p.dir,
                &p.state_dir,
                child_args,
            );
            if !self.exec_shell_tail && inner.ends_with("; exec bash") {
                let cut = inner.len() - "; exec bash".len();
                inner.truncate(cut);
            }
            reject_newlines(&inner, "mintty inner command")?;
            let st = Command::new(&mt)
                .arg("-e")
                .arg("bash")
                .arg("-lc")
                .arg(&inner)
                .status();
            match st {
                Ok(s) if s.success() => {}
                _ => return Err("failed to launch a mintty window".to_string()),
            }
        }
        Ok(())
    }

    fn supports_post_merge(&self) -> bool {
        // Non-waitable generally
        false
    }
}
