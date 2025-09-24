
//! Data structures used across fork orchestration.
//!
//! Invariants
//! - ForkSession captures immutable creation-time facts (sid, base refs, layout, agent).
//! - Pane indexes are 1-based; container names and state dirs derive from (agent, sid, index).
//! - ForkOptions is a read-only snapshot for runner coordination; do not mutate during runs.
//!
//! Note: keep these types small and self-explanatory; avoid embedding behavior.

use aifo_coder::MergingStrategy;
use std::path::PathBuf;

/// High-level fork session information captured at creation time.
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    pub merge_strategy: MergingStrategy,
    pub autoclean: bool,
    pub dry_run: bool,
    pub include_dirty: bool,
    pub dissociate: bool,
}

impl ForkOptions {
    /// Touch all fields so clippy sees them as read, without changing behavior.
    pub fn touch(&self) {
        let _ = (
            self.verbose,
            self.keep_on_failure,
            self.autoclean,
            self.dry_run,
            self.include_dirty,
            self.dissociate,
        );
        let _ = match self.merge_strategy {
            MergingStrategy::None => 0,
            MergingStrategy::Fetch => 1,
            MergingStrategy::Octopus => 2,
        };
    }
}
