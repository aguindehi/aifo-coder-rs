//! Build child argument vector for panes launched by fork orchestrators.
//!
//! Semantics
//! - Preserves agent subcommand and its args verbatim.
//! - Strips fork-only flags (layout, session, dissociate, merge options).
//! - Includes global flags (image/flavor/toolchains/verbosity) as the agent expects.
//!
//! Note: keep golden-sensitive strings intact; tests assert ordering and presence of specific flags.

pub(crate) fn fork_build_child_args(cli: &crate::cli::Cli) -> Vec<String> {
    use crate::cli::{Agent, Flavor};

    let mut args: Vec<String> = Vec::new();

    if let Some(img) = cli.image.as_deref() {
        if !img.trim().is_empty() {
            args.push("--image".to_string());
            args.push(img.to_string());
        }
    }
    for k in &cli.toolchain {
        args.push("--toolchain".to_string());
        args.push(k.as_str().to_string());
    }
    if cli.no_toolchain_cache {
        args.push("--no-toolchain-cache".to_string());
    }
    if cli.toolchain_unix_socket {
        args.push("--toolchain-unix-socket".to_string());
    }
    for b in &cli.toolchain_bootstrap {
        args.push("--toolchain-bootstrap".to_string());
        args.push(b.clone());
    }
    if cli.verbose {
        args.push("--verbose".to_string());
    }
    if let Some(fl) = cli.flavor {
        args.push("--flavor".to_string());
        args.push(
            match fl {
                Flavor::Full => "full",
                Flavor::Slim => "slim",
            }
            .to_string(),
        );
    }
    if cli.invalidate_registry_cache {
        args.push("--invalidate-registry-cache".to_string());
    }
    if cli.dry_run {
        args.push("--dry-run".to_string());
    }

    // Subcommand and its args
    match &cli.command {
        Agent::Codex { args: a } => {
            args.push("codex".to_string());
            args.extend(a.clone());
        }
        Agent::Crush { args: a } => {
            args.push("crush".to_string());
            args.extend(a.clone());
        }
        Agent::Aider { args: a } => {
            args.push("aider".to_string());
            args.extend(a.clone());
        }
        // For non-agent subcommands, default to aider to avoid starting doctor/images in panes.
        _ => {
            args.push("aider".to_string());
        }
    }

    args
}

#[cfg(test)]
mod args_tests {
    use super::*;
    use clap::Parser;

    fn make_cli_for_test() -> crate::cli::Cli {
        crate::cli::Cli {
            image: Some("example.com/org/agent:tag".to_string()),
            toolchain: vec![
                "rust".parse().expect("valid toolchain spec"),
                "node".parse().expect("valid toolchain spec"),
                "python@3.12".parse().expect("valid toolchain spec"),
                "go=golang:1.22-bookworm"
                    .parse()
                    .expect("valid toolchain spec"),
            ],
            no_toolchain_cache: true,
            toolchain_unix_socket: false,
            toolchain_bootstrap: vec!["typescript=global".to_string()],
            verbose: true,
            debug_otel_otlp: false,
            quiet: false,
            ignore_local_images: false,
            non_interactive: false,
            flavor: Some(crate::cli::Flavor::Slim),
            invalidate_registry_cache: false,
            dry_run: true,
            // fork flags that must be stripped from child args
            fork: Some(3),
            fork_include_dirty: true,
            fork_dissociate: true,
            fork_session_name: Some("ut-session".to_string()),
            fork_layout: Some("even-h".to_string()),
            fork_keep_on_failure: true,
            fork_merging_strategy: aifo_coder::MergingStrategy::None,
            fork_merging_autoclean: false,
            color: Some(aifo_coder::ColorMode::Auto),
            command: crate::cli::Agent::Aider {
                args: vec!["--help".to_string(), "--".to_string(), "extra".to_string()],
            },
        }
    }

    #[test]
    fn test_fork_build_child_args_strips_fork_flags_and_preserves_agent_args() {
        let cli = make_cli_for_test();
        let args = fork_build_child_args(&cli);
        // Must contain agent subcommand (root flags precede it)
        assert!(
            args.iter().any(|s| s == "aider"),
            "child args must contain agent subcommand, got: {:?}",
            args
        );
        // Must include some global flags we set
        let joined = args.join(" ");
        assert!(
            joined.contains("--image example.com/org/agent:tag"),
            "expected --image in child args: {}",
            joined
        );
        assert!(
            joined.contains("--toolchain rust")
                && joined.contains("--toolchain node")
                && joined.contains("--toolchain python@3.12")
                && joined.contains("--toolchain go=golang:1.22-bookworm"),
            "expected --toolchain specs in child args: {}",
            joined
        );
        assert!(
            !joined.contains("--toolchain-spec"),
            "unexpected legacy flag --toolchain-spec in child args: {}",
            joined
        );
        assert!(
            !joined.contains("--toolchain-image"),
            "unexpected legacy flag --toolchain-image in child args: {}",
            joined
        );
        assert!(
            joined.contains("--no-toolchain-cache"),
            "expected --no-toolchain-cache in child args: {}",
            joined
        );
        assert!(
            joined.contains("--flavor slim"),
            "expected --flavor slim in child args: {}",
            joined
        );
        assert!(
            joined.contains("--dry-run"),
            "expected --dry-run in child args: {}",
            joined
        );
        // Must NOT contain any fork flags
        for bad in [
            "--fork ",
            "--fork-include-dirty",
            "--fork-dissociate",
            "--fork-session-name",
            "--fork-layout",
            "--fork-keep-on-failure",
            "--fork-merge-strategy",
            "--fork-merge-no-autoclean",
        ] {
            assert!(
                !joined.contains(bad),
                "fork flag leaked into child args: {} in {}",
                bad,
                joined
            );
        }
        // Agent args should be preserved; literal '--' boundary must be present and ordering intact
        assert!(
            args.contains(&"--".to_string()),
            "expected literal '--' argument to be present in child args: {:?}",
            args
        );
        let idx_boundary = args.iter().position(|s| s == "--");
        let idx_extra = args.iter().position(|s| s == "extra");
        assert!(
            idx_boundary.is_some() && idx_extra.is_some() && idx_boundary < idx_extra,
            "expected '--' to appear before trailing agent args, got: {:?}",
            args
        );
        let dashdash_count = args.iter().filter(|s| s.as_str() == "--").count();
        assert_eq!(
            dashdash_count, 1,
            "expected exactly one '--' token, got: {:?}",
            args
        );
        assert!(
            joined.contains("aider --help"),
            "expected agent arg '--help' to be present: {}",
            joined
        );
        assert!(
            joined.contains("extra"),
            "expected trailing agent arg to be present: {}",
            joined
        );
    }

    #[test]
    fn test_merging_strategy_value_enum_parsing() {
        let cli = crate::cli::Cli::parse_from([
            "aifo-coder",
            "--fork-merge-strategy",
            "octopus",
            "aider",
            "--",
            "--help",
        ]);
        assert!(
            matches!(
                cli.fork_merging_strategy,
                aifo_coder::MergingStrategy::Octopus
            ),
            "expected parsing of --fork-merge-strategy octopus"
        );
    }

    #[test]
    fn test_non_agent_subcommands_default_to_aider_in_child_args() {
        // Images subcommand should result in aider child args
        let cli = crate::cli::Cli::parse_from(["aifo-coder", "images"]);
        let args = fork_build_child_args(&cli);
        assert!(
            args.iter().any(|s| s == "aider"),
            "expected aider in child args for non-agent subcommand, got: {:?}",
            args
        );

        // Doctor subcommand should also default to aider in child args
        let cli2 = crate::cli::Cli::parse_from(["aifo-coder", "doctor"]);
        let args2 = fork_build_child_args(&cli2);
        assert!(
            args2.iter().any(|s| s == "aider"),
            "expected aider in child args for doctor subcommand, got: {:?}",
            args2
        );
    }
}
