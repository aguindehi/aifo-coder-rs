#![cfg(not(windows))]

use std::fs;
use std::process::Command;

use super::super::types::{ForkSession, Pane};
use super::Orchestrator;

/// tmux orchestrator (Unix): creates a session, splits panes, sets layout, and launches per-pane scripts.
/// Waits for attach/switch to complete before returning (i.e., after user detaches).
pub struct Tmux;

impl Orchestrator for Tmux {
    fn launch(
        &self,
        session: &ForkSession,
        panes: &[Pane],
        child_args: &[String],
    ) -> Result<(), String> {
        let tmux = which::which("tmux").map_err(|_| "tmux not found".to_string())?;

        // Build child command string: aifo-coder plus child args
        let mut words = vec!["aifo-coder".to_string()];
        words.extend(child_args.iter().cloned());
        let child_joined = aifo_coder::shell_join(&words);

        // Create new session with first pane's directory
        let (first_dir, _) = panes
            .first()
            .map(|p| (p.dir.clone(), p.branch.clone()))
            .ok_or_else(|| "no panes to create".to_string())?;
        {
            let mut cmd = Command::new(&tmux);
            cmd.arg("new-session")
                .arg("-d")
                .arg("-s")
                .arg(&session.session_name)
                .arg("-n")
                .arg("aifo-fork")
                .arg("-c")
                .arg(&first_dir);
            let st = cmd.status().map_err(|e| e.to_string())?;
            if !st.success() {
                return Err("tmux new-session failed".to_string());
            }
        }

        // Split remaining panes
        for (idx, p) in panes.iter().enumerate().skip(1) {
            let mut cmd = Command::new(&tmux);
            cmd.arg("split-window")
                .arg("-t")
                .arg(format!("{}:0", &session.session_name))
                .arg("-c")
                .arg(&p.dir);
            let st = cmd.status().map_err(|e| e.to_string())?;
            if !st.success() {
                // Best-effort cleanup
                let _ = Command::new(&tmux)
                    .arg("kill-session")
                    .arg("-t")
                    .arg(&session.session_name)
                    .status();
                return Err(format!("tmux split-window failed for pane {}", idx + 1));
            }
        }

        // Apply layout mapping
        let layout_effective = match session.layout.as_str() {
            "even-h" => "even-horizontal",
            "even-v" => "even-vertical",
            _ => "tiled",
        };
        let _ = Command::new(&tmux)
            .arg("select-layout")
            .arg("-t")
            .arg(format!("{}:0", &session.session_name))
            .arg(layout_effective)
            .status();

        // Synchronize panes off
        let _ = Command::new(&tmux)
            .arg("set-window-option")
            .arg("-t")
            .arg(format!("{}:0", &session.session_name))
            .arg("synchronize-panes")
            .arg("off")
            .status();

        // Prepare and send per-pane launch scripts
        for (idx, p) in panes.iter().enumerate() {
            let container_name =
                aifo_coder::fork::env::pane_container_name(&session.agent, &session.sid, p.index);
            let script = aifo_coder::fork::inner::build_tmux_launch_script(
                &session.sid,
                p.index,
                &container_name,
                &p.state_dir,
                &child_joined,
                "/launcher",
            );
            let script_path = p.state_dir.join("launch.sh");
            let _ = fs::create_dir_all(&p.state_dir);
            let _ = fs::write(&script_path, script.as_bytes());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700));
            }
            let target = format!("{}:0.{}", &session.session_name, idx);
            let shwrap = format!(
                "sh -lc {}",
                aifo_coder::shell_escape(&script_path.display().to_string())
            );
            let _ = Command::new(&tmux)
                .arg("send-keys")
                .arg("-t")
                .arg(target)
                .arg(shwrap)
                .arg("C-m")
                .status();
        }

        // Attach or switch
        let attach_cmd = if std::env::var("TMUX")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some()
        {
            vec![
                "switch-client".to_string(),
                "-t".to_string(),
                session.session_name.clone(),
            ]
        } else {
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                session.session_name.clone(),
            ]
        };
        let mut att = Command::new(&tmux);
        for a in &attach_cmd {
            att.arg(a);
        }
        let _ = att.status();

        Ok(())
    }

    fn supports_post_merge(&self) -> bool {
        true
    }
}
