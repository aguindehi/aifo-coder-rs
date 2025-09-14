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
