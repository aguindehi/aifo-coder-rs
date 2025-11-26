use clap::{Parser, Subcommand};

/// Validate tmux layout flag value
fn validate_layout(s: &str) -> Result<String, String> {
    match s {
        "tiled" | "even-h" | "even-v" => Ok(s.to_string()),
        _ => Err("must be one of tiled, even-h, even-v".to_string()),
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, clap::ValueEnum)]
pub(crate) enum Flavor {
    Full,
    Slim,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, clap::ValueEnum)]
pub(crate) enum ToolchainKind {
    Rust,
    Node,
    #[value(alias = "ts")]
    Typescript,
    Python,
    #[value(alias = "ccpp")]
    #[value(alias = "c")]
    #[value(alias = "cpp")]
    #[value(alias = "c_cpp")]
    #[value(alias = "c++")]
    CCpp,
    Go,
}

impl ToolchainKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            ToolchainKind::Rust => "rust",
            ToolchainKind::Node => "node",
            ToolchainKind::Typescript => "typescript",
            ToolchainKind::Python => "python",
            ToolchainKind::CCpp => "c-cpp",
            ToolchainKind::Go => "go",
        }
    }
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum ForkCmd {
    /// List existing fork sessions under the current repo
    List {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
        /// Scan across repositories under AIFO_CODER_WORKSPACE_ROOT (non-recursive; requires env)
        #[arg(long = "all-repos")]
        all_repos: bool,
        /// Colorize output: auto|always|never
        #[arg(long = "color", value_enum)]
        color: Option<aifo_coder::ColorMode>,
    },
    /// Clean fork sessions and panes with safety protections
    Clean {
        /// Target a single session id
        #[arg(long = "session")]
        session: Option<String>,
        /// Target sessions older than N days
        #[arg(long = "older-than")]
        older_than: Option<u64>,
        /// Target all sessions
        #[arg(long = "all")]
        all: bool,
        /// Print what would be done without deleting
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Proceed without interactive confirmation
        #[arg(long = "yes")]
        yes: bool,
        /// Override safety protections and delete everything
        #[arg(long = "force")]
        force: bool,
        /// Delete only clean panes; keep dirty/ahead/base-unknown
        #[arg(long = "keep-dirty")]
        keep_dirty: bool,
        /// Emit machine-readable JSON summary (plan in --dry-run; result when executed)
        #[arg(long)]
        json: bool,
    },

    /// Merge fork panes back into the original repository
    Merge {
        /// Session id to merge
        #[arg(long = "session")]
        session: String,
        /// Strategy: none|fetch|octopus
        #[arg(long = "strategy", value_enum)]
        strategy: aifo_coder::MergingStrategy,
        /// Automatically dispose the fork session after a successful octopus merge
        #[arg(long = "autoclean")]
        autoclean: bool,
        /// Print what would be done without modifying
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum Agent {
    /// Run diagnostics to check environment and configuration
    Doctor,

    /// Run support matrix for coder/toolchains
    Support {
        /// Run baseline checks only
        #[arg(long = "base")]
        base: bool,
        /// Run deep checks only
        #[arg(long = "deep")]
        deep: bool,
        /// Run combo checks only
        #[arg(long = "combo")]
        combo: bool,
        /// Run all support matrix modes (baseline, deep, combo) in sequence
        /// Default when no --base/--deep/--combo are provided
        #[arg(long = "all")]
        all: bool,
    },

    /// Show effective image references (including flavor/registry)
    Images,

    /// Clear on-disk caches (e.g., registry probe cache)
    CacheClear,

    /// Purge all named toolchain cache volumes (cargo, npm, pip, ccache, go)
    ToolchainCacheClear,

    /// Toolchain sidecar: run a command inside a language toolchain sidecar
    Toolchain {
        #[arg(value_enum)]
        kind: ToolchainKind,
        /// Override the toolchain image reference for this run
        #[arg(long = "toolchain-image")]
        image: Option<String>,
        /// Disable named cache volumes for the toolchain sidecar
        #[arg(long = "no-toolchain-cache")]
        no_cache: bool,
        /// Command and arguments to execute inside the sidecar (after --)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Run OpenAI Codex CLI
    Codex {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run Charmbracelet Crush
    Crush {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run Aider
    Aider {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run OpenHands
    #[command(name = "openhands")]
    OpenHands {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run OpenCode
    #[command(name = "opencode")]
    OpenCode {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run Plandex
    Plandex {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Fork maintenance commands
    #[command(
        after_long_help = "Examples:\n  aifo-coder fork list --json\n  aifo-coder fork clean --session abc123 --dry-run --json\n  aifo-coder fork clean --older-than 30 --yes --keep-dirty\n  aifo-coder fork merge --session abc123 --strategy octopus --autoclean\n"
    )]
    Fork {
        #[command(subcommand)]
        cmd: ForkCmd,
    },
}

#[derive(Parser, Debug)]
#[command(
    name = "aifo-coder",
    version,
    about = "Run Codex, Crush, Aider, Opencode, Openhands or Plandex inside Docker with current directory mounted.",
    override_usage = "aifo-coder [OPTIONS] <COMMAND> [-- [AGENT-OPTIONS]]",
    after_long_help = "Examples:\n  aifo-coder --fork 2 aider -- --help\n  aifo-coder --fork 3 --fork-include-dirty --fork-session-name aifo-work aider --\n  aifo-coder fork list --json\n  aifo-coder fork clean --older-than 14 --yes\n\n",
    after_help = "\n"
)]
pub(crate) struct Cli {
    /// Override Docker image (full ref). If unset, use per-agent default: {prefix}-{agent}:{tag}
    #[arg(long)]
    pub(crate) image: Option<String>,

    /// Attach language toolchains and inject PATH shims (repeatable)
    #[arg(long = "toolchain", value_enum)]
    pub(crate) toolchain: Vec<ToolchainKind>,

    /// Attach toolchains with optional versions (repeatable), e.g. rust@1.80, node@20, python@3.12
    #[arg(long = "toolchain-spec")]
    pub(crate) toolchain_spec: Vec<String>,

    /// Override image(s) for toolchains (repeatable, kind=image)
    #[arg(long = "toolchain-image")]
    pub(crate) toolchain_image: Vec<String>,

    /// Disable named cache volumes for toolchain sidecars
    #[arg(long = "no-toolchain-cache")]
    pub(crate) no_toolchain_cache: bool,

    /// Use Linux unix socket transport for tool-exec proxy (instead of TCP)
    #[arg(long = "toolchain-unix-socket")]
    pub(crate) toolchain_unix_socket: bool,

    /// Bootstrap actions for toolchains (repeatable), e.g. typescript=global
    #[arg(long = "toolchain-bootstrap")]
    pub(crate) toolchain_bootstrap: Vec<String>,

    /// Print detailed execution info
    #[arg(long)]
    pub(crate) verbose: bool,

    /// Suppress startup banner output
    #[arg(long, short = 'q')]
    pub(crate) quiet: bool,

    /// Choose image flavor: full or slim (overrides AIFO_CODER_IMAGE_FLAVOR)
    #[arg(long, value_enum)]
    pub(crate) flavor: Option<Flavor>,

    /// Invalidate on-disk registry cache before probing
    #[arg(long)]
    pub(crate) invalidate_registry_cache: bool,

    /// Prepare and print what would run, but do not execute
    #[arg(long)]
    pub(crate) dry_run: bool,

    /// Disable interactive LLM prompt (same as AIFO_CODER_SUPPRESS_LLM_WARNING=1)
    #[arg(long = "non-interactive", alias = "no-llm-prompt")]
    pub(crate) non_interactive: bool,

    /// Colorize output: auto|always|never
    #[arg(long = "color", value_enum)]
    pub(crate) color: Option<aifo_coder::ColorMode>,

    /// Fork mode: create N panes (N>=2) in tmux/Windows Terminal with cloned workspaces
    #[arg(long)]
    pub(crate) fork: Option<usize>,

    /// Include uncommitted changes via snapshot commit (temporary index + commit-tree; no hooks/signing)
    #[arg(long = "fork-include-dirty")]
    pub(crate) fork_include_dirty: bool,

    /// Clone with --dissociate for independence
    #[arg(long = "fork-dissociate")]
    pub(crate) fork_dissociate: bool,

    /// Session/window name override
    #[arg(long = "fork-session-name")]
    pub(crate) fork_session_name: Option<String>,

    /// Layout for tmux panes: tiled, even-h, or even-v
    #[arg(long = "fork-layout", value_parser = validate_layout)]
    pub(crate) fork_layout: Option<String>,

    /// Keep created clones on orchestration failure (default: keep)
    #[arg(long = "fork-keep-on-failure", default_value_t = true)]
    pub(crate) fork_keep_on_failure: bool,

    /// Post-fork merge strategy to apply after all panes exit (default: octopus)
    #[arg(long = "fork-merge-strategy", value_enum, default_value_t = aifo_coder::MergingStrategy::Octopus, hide_default_value = true)]
    pub(crate) fork_merging_strategy: aifo_coder::MergingStrategy,

    /// Disable automatic disposal of the fork session after a successful octopus merge (default: enabled)
    #[arg(long = "fork-merge-no-autoclean", default_value_t = true, action = clap::ArgAction::SetFalse)]
    pub(crate) fork_merging_autoclean: bool,

    #[command(subcommand)]
    pub(crate) command: Agent,
}
