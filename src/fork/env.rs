use std::path::PathBuf;

use super::types::{ForkSession, Pane};

/// Build environment key/value pairs for a fork pane (for use in orchestrators).
/// Includes AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 and AIFO_CODER_SKIP_LOCK=1.
pub fn fork_env_for_pane(session: &ForkSession, pane: &Pane) -> Vec<(String, String)> {
    let cname = pane.container_name.clone();
    vec![
        ("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING".into(), "1".into()),
        ("AIFO_CODER_SKIP_LOCK".into(), "1".into()),
        ("AIFO_CODER_CONTAINER_NAME".into(), cname.clone()),
        ("AIFO_CODER_HOSTNAME".into(), cname),
        ("AIFO_CODER_FORK_SESSION".into(), session.sid.clone()),
        ("AIFO_CODER_FORK_INDEX".into(), pane.index.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR".into(),
            pane.state_dir.display().to_string(),
        ),
    ]
}

/// Convenience to compute a pane container name consistently.
pub fn pane_container_name(agent: &str, sid: &str, index: usize) -> String {
    format!("aifo-coder-{}-{}-{}", agent, sid, index)
}

/// Compute a pane state directory path under the given base.
pub fn pane_state_dir(state_base: &PathBuf, sid: &str, index: usize) -> PathBuf {
    state_base.join(sid).join(format!("pane-{}", index))
}
