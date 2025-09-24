#![cfg(windows)]

use std::process::Command;

use super::super::types::{ForkSession, Pane};
use super::Orchestrator;

/// Windows Terminal orchestrator (non-waitable).
pub struct WindowsTerminal;

impl Orchestrator for WindowsTerminal {
    fn launch(
        &self,
        session: &ForkSession,
        panes: &[Pane],
        child_args: &[String],
    ) -> Result<(), String> {
        let wt_path = which::which("wt")
            .or_else(|_| which::which("wt.exe"))
            .map_err(|_| "Windows Terminal (wt.exe) not found in PATH".to_string())?;

        // PowerShell to run in each pane
        let psbin = which::which("pwsh")
            .or_else(|_| which::which("powershell"))
            .or_else(|_| which::which("powershell.exe"))
            .unwrap_or_else(|_| std::path::PathBuf::from("powershell"));

        // Pane 1: new-tab
        if let Some(first) = panes.first() {
            let inner = aifo_coder::fork_ps_inner_string(
                &session.agent,
                &session.sid,
                first.index,
                &first.dir,
                &first.state_dir,
                child_args,
            );
            let args = aifo_coder::wt_build_new_tab_args(&psbin, &first.dir, &inner);
            let mut cmd = Command::new(&wt_path);
            for a in args.iter().skip(1) {
                cmd.arg(a);
            }
            let st = cmd.status().map_err(|e| e.to_string())?;
            if !st.success() {
                return Err("Windows Terminal failed to start first pane".to_string());
            }
        } else {
            return Err("no panes to create".to_string());
        }

        // Remaining panes: split-pane with orientation based on layout
        for p in panes.iter().skip(1) {
            let inner = aifo_coder::fork_ps_inner_string(
                &session.agent,
                &session.sid,
                p.index,
                &p.dir,
                &p.state_dir,
                child_args,
            );
            let orient = aifo_coder::wt_orient_for_layout(&session.layout, p.index);
            let args = aifo_coder::wt_build_split_args(orient, &psbin, &p.dir, &inner);
            let mut cmd = Command::new(&wt_path);
            for a in args.iter().skip(1) {
                cmd.arg(a);
            }
            let st = cmd.status().map_err(|e| e.to_string())?;
            if !st.success() {
                return Err("Windows Terminal split-pane failed".to_string());
            }
        }

        Ok(())
    }

    fn supports_post_merge(&self) -> bool {
        // Detached, non-waitable
        false
    }
}
