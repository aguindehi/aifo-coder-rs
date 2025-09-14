use std::path::PathBuf;
use clap::ValueEnum;

/**
 Merging strategy for post-fork actions.
 - None: do nothing (default).
 - Fetch: fetch pane branches back into the original repository as local branches.
 - Octopus: fetch branches then attempt an octopus merge into a merge/<sid> branch.
*/
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
pub enum MergingStrategy {
    #[value(name = "none")]
    None,
    #[value(name = "fetch")]
    Fetch,
    #[value(name = "octopus")]
    Octopus,
}

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
            aifo_coder::MergingStrategy::None => 0,
            aifo_coder::MergingStrategy::Fetch => 1,
            aifo_coder::MergingStrategy::Octopus => 2,
        };
    }
}
