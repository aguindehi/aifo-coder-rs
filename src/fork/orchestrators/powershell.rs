#![cfg(windows)]

use std::path::PathBuf;
use std::process::Command;

use super::super::types::{ForkSession, Pane};
use super::Orchestrator;

/// PowerShell orchestrator: opens one window per pane using Start-Process.
/// When `wait` is true, waits for all spawned PIDs to exit before returning.
pub struct PowerShell {
    pub wait: bool,
}

impl Orchestrator for PowerShell {
    fn launch(
        &self,
        session: &ForkSession,
        panes: &[Pane],
        child_args: &[String],
    ) -> Result<(), String> {
        let ps_path = which::which("pwsh")
            .or_else(|_| which::which("powershell"))
            .or_else(|_| which::which("powershell.exe"))
            .map_err(|_| "PowerShell not found in PATH".to_string())?;

        let mut pids: Vec<String> = Vec::new();

        for p in panes {
            let inner = aifo_coder::fork_ps_inner_string(
                &session.agent,
                &session.sid,
                p.index,
                &p.dir,
                &p.state_dir,
                child_args,
            );
            let wd = quote_ps(&p.dir);
            let child = quote_ps(&ps_path);
            let inner_q = quote_literal(&inner);
            // Keep -NoExit only when not waiting (so window remains open)
            let arglist = if self.wait {
                "'-Command'".to_string()
            } else {
                "'-NoExit','-Command'".to_string()
            };
            let script = format!(
                "(Start-Process -WindowStyle Normal -WorkingDirectory {wd} {child} -ArgumentList {arglist},{inner} -PassThru).Id",
                wd = wd,
                child = child,
                arglist = arglist,
                inner = inner_q
            );
            let out = Command::new(&ps_path)
                .arg("-NoProfile")
                .arg("-Command")
                .arg(&script)
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                let pid = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if pid.is_empty() {
                    println!("[{}] started dir={} (PID unknown)", p.index, p.dir.display());
                } else {
                    println!("[{}] started PID={} dir={}", p.index, pid, p.dir.display());
                    pids.push(pid);
                }
            } else {
                return Err("failed to launch a PowerShell window".to_string());
            }
        }

        if self.wait && !pids.is_empty() {
            let wait_cmd = format!("Wait-Process -Id {}", pids.join(","));
            let _ = Command::new(&ps_path)
                .arg("-NoProfile")
                .arg("-Command")
                .arg(wait_cmd)
                .status();
        }

        Ok(())
    }

    fn supports_post_merge(&self) -> bool {
        // We can wait for panes to exit
        true
    }
}

fn quote_ps(p: &PathBuf) -> String {
    let s = p.display().to_string();
    quote_literal(&s)
}

fn quote_literal(s: &str) -> String {
    let esc = s.replace('\'', "''");
    format!("'{}'", esc)
}
