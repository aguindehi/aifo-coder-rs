use std::path::PathBuf;

/// High-level fork session information captured at creation time.
pub struct ForkSession {
    pub sid: String,
    pub session_name: String,
    pub base_label: String,
    pub base_ref_or_sha: String,
    pub base_commit_sha: String,
    pub created_at: u64,
    pub layout: String,
    pub agent: String,
    pub session_dir: PathBuf,
}

/// A single pane description.
pub struct Pane {
    pub index: usize,
    pub dir: PathBuf,
    pub branch: String,
    pub state_dir: PathBuf,
    pub container_name: String,
}

/// Options snapshot used during fork orchestration.
pub struct ForkOptions {
    pub verbose: bool,
    pub keep_on_failure: bool,
    pub merge_strategy: aifo_coder::MergingStrategy,
    pub autoclean: bool,
    pub dry_run: bool,
    pub include_dirty: bool,
    pub dissociate: bool,
}
