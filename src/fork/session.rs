use crate::fork::types::{ForkSession, Pane};
use std::path::{Path, PathBuf};

/// Build a ForkSession from the common fields used throughout fork orchestration.
pub fn make_session(
    sid: &str,
    session_name: &str,
    base_label: &str,
    base_ref_or_sha: &str,
    base_commit_sha: &str,
    created_at: u64,
    layout: &str,
    agent: &str,
    session_dir: &Path,
) -> ForkSession {
    ForkSession {
        sid: sid.to_string(),
        session_name: session_name.to_string(),
        base_label: base_label.to_string(),
        base_ref_or_sha: base_ref_or_sha.to_string(),
        base_commit_sha: base_commit_sha.to_string(),
        created_at,
        layout: layout.to_string(),
        agent: agent.to_string(),
        session_dir: session_dir.to_path_buf(),
    }
}

/// Build a Pane from the common fields used throughout fork orchestration.
pub fn make_pane(
    index: usize,
    dir: &Path,
    branch: &str,
    state_dir: &Path,
    container_name: &str,
) -> Pane {
    Pane {
        index,
        dir: dir.to_path_buf(),
        branch: branch.to_string(),
        state_dir: state_dir.to_path_buf(),
        container_name: container_name.to_string(),
    }
}
