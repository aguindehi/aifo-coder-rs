pub mod types;
pub mod env;
pub mod inner;
pub mod meta;
pub mod orchestrators;
pub mod post_merge;

/// Return the selected agent subcommand as a static str ("aider" | "crush" | "codex").
/// Defaults to "aider" for non-agent subcommands to preserve current behavior.
pub fn select_agent_str(cli: &crate::cli::Cli) -> &'static str {
    use crate::cli::Agent;
    match &cli.command {
        Agent::Codex { .. } => "codex",
        Agent::Crush { .. } => "crush",
        Agent::Aider { .. } => "aider",
        _ => "aider",
    }
}
