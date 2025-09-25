//! Environment helpers for fork panes and inner builders.
//!
//! Invariants
//! - AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 is exported to panes (suppress startup hints).
//! - AIFO_CODER_SKIP_LOCK=1 avoids acquiring the repo lock in child panes.
//! - Per-pane state directory (AIFO_CODER_FORK_STATE_DIR) is used by orchestrators and shells.
//! - Container naming is stable: aifo-coder-<agent>-<sid>-<index>.
//!
//! Windows inner builders exclude SUPPRESS by design; orchestrators inject SUPPRESS in the pane env.

use std::path::{Path, PathBuf};

/// Build environment key/value pairs for a fork pane (for use in orchestrators).
/// Includes AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING=1 and AIFO_CODER_SKIP_LOCK=1.
pub fn fork_env_for_pane(
    sid: &str,
    pane_index: usize,
    container_name: &str,
    pane_state_dir: &Path,
) -> Vec<(String, String)> {
    vec![
        ("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING".into(), "1".into()),
        ("AIFO_CODER_SKIP_LOCK".into(), "1".into()),
        (
            "AIFO_CODER_CONTAINER_NAME".into(),
            container_name.to_string(),
        ),
        ("AIFO_CODER_HOSTNAME".into(), container_name.to_string()),
        ("AIFO_CODER_FORK_SESSION".into(), sid.to_string()),
        ("AIFO_CODER_FORK_INDEX".into(), pane_index.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR".into(),
            pane_state_dir.display().to_string(),
        ),
    ]
}

/// Convenience to compute a pane container name consistently.
pub fn pane_container_name(agent: &str, sid: &str, index: usize) -> String {
    format!("aifo-coder-{}-{}-{}", agent, sid, index)
}

/// Compute a pane state directory path under the given base.
pub fn pane_state_dir(state_base: &Path, sid: &str, index: usize) -> PathBuf {
    state_base.join(sid).join(format!("pane-{}", index))
}

/// Build environment key/value pairs used by inner string builders (PowerShell/Git Bash).
/// Excludes AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING by design; orchestrators inject it in the pane env.
#[cfg(windows)]
pub(crate) fn fork_inner_env_kv(
    agent: &str,
    sid: &str,
    i: usize,
    pane_state_dir: &Path,
) -> Vec<(String, String)> {
    let cname = pane_container_name(agent, sid, i);
    vec![
        ("AIFO_CODER_SKIP_LOCK".into(), "1".into()),
        ("AIFO_CODER_CONTAINER_NAME".into(), cname.clone()),
        ("AIFO_CODER_HOSTNAME".into(), cname),
        ("AIFO_CODER_FORK_SESSION".into(), sid.to_string()),
        ("AIFO_CODER_FORK_INDEX".into(), i.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR".into(),
            pane_state_dir.display().to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_fork_env_contains_expected_keys() {
        let sid = "sid-ut";
        let pane_index = 2usize;
        let container_name = "aifo-coder-aider-sid-ut-2";
        let state_dir = PathBuf::from("./state/p2");

        let envs = fork_env_for_pane(sid, pane_index, container_name, &state_dir);

        let get = |k: &str| -> Option<String> {
            envs.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone())
        };

        assert_eq!(
            get("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING").as_deref(),
            Some("1")
        );
        assert_eq!(get("AIFO_CODER_SKIP_LOCK").as_deref(), Some("1"));
        assert_eq!(
            get("AIFO_CODER_CONTAINER_NAME"),
            Some(container_name.to_string())
        );
        assert_eq!(get("AIFO_CODER_HOSTNAME"), Some(container_name.to_string()));
        assert_eq!(get("AIFO_CODER_FORK_SESSION"), Some(sid.to_string()));
        assert_eq!(get("AIFO_CODER_FORK_INDEX").as_deref(), Some("2"));
        assert_eq!(
            get("AIFO_CODER_FORK_STATE_DIR").as_deref(),
            Some(state_dir.display().to_string().as_str())
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_fork_inner_env_kv_excludes_suppress_and_has_expected_keys() {
        let agent = "aider";
        let sid = "sid-ut";
        let i = 2usize;
        let dir = std::path::Path::new("./state/p2");
        let kv = fork_inner_env_kv(agent, sid, i, dir);

        let get = |k: &str| -> Option<String> {
            kv.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone())
        };

        // Excludes SUPPRESS var
        assert!(
            !kv.iter()
                .any(|(k, _)| k == "AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING"),
            "inner env kv must not include SUPPRESS var"
        );

        // Includes expected keys and values
        assert_eq!(get("AIFO_CODER_SKIP_LOCK").as_deref(), Some("1"));
        assert_eq!(
            get("AIFO_CODER_CONTAINER_NAME").as_deref(),
            Some("aifo-coder-aider-sid-ut-2")
        );
        assert_eq!(
            get("AIFO_CODER_HOSTNAME").as_deref(),
            Some("aifo-coder-aider-sid-ut-2")
        );
        assert_eq!(get("AIFO_CODER_FORK_SESSION").as_deref(), Some("sid-ut"));
        assert_eq!(get("AIFO_CODER_FORK_INDEX").as_deref(), Some("2"));
        assert_eq!(
            get("AIFO_CODER_FORK_STATE_DIR").as_deref(),
            Some(dir.display().to_string().as_str())
        );
    }
}
