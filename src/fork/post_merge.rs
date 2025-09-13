use std::path::Path;

/// Apply post-merge with standardized logging. When `plain` is true, print
/// non-colored success/failure and octopus-start messages; the initial "applying"
/// line remains color-aware to match existing behavior.
pub fn apply_post_merge(
    repo_root: &Path,
    sid: &str,
    strategy: aifo_coder::MergingStrategy,
    autoclean: bool,
    dry_run: bool,
    verbose: bool,
    plain: bool,
) -> Result<(), String> {
    if matches!(strategy, aifo_coder::MergingStrategy::None) {
        return Ok(());
    }
    let strat = match strategy {
        aifo_coder::MergingStrategy::None => "none",
        aifo_coder::MergingStrategy::Fetch => "fetch",
        aifo_coder::MergingStrategy::Octopus => "octopus",
    };
    {
        let use_err = aifo_coder::color_enabled_stderr();
        eprintln!(
            "{}",
            aifo_coder::paint(
                use_err,
                "\x1b[36;1m",
                &format!("aifo-coder: applying post-fork merge strategy: {}", strat)
            )
        );
    }
    match aifo_coder::fork_merge_branches_by_session(repo_root, sid, strategy, verbose, dry_run) {
        Ok(()) => {
            if plain {
                eprintln!("aifo-coder: merge strategy '{}' completed.", strat);
            } else {
                let use_err = aifo_coder::color_enabled_stderr();
                eprintln!(
                    "{}",
                    aifo_coder::paint(
                        use_err,
                        "\x1b[32;1m",
                        &format!("aifo-coder: merge strategy '{}' completed.", strat)
                    )
                );
            }
            if matches!(strategy, aifo_coder::MergingStrategy::Octopus) && autoclean && !dry_run {
                eprintln!();
                if plain {
                    eprintln!(
                        "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                        sid
                    );
                } else {
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[36;1m",
                            &format!(
                                "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                sid
                            )
                        )
                    );
                }
                let opts = aifo_coder::ForkCleanOpts {
                    session: Some(sid.to_string()),
                    older_than_days: None,
                    all: false,
                    dry_run: false,
                    yes: true,
                    force: true,
                    keep_dirty: false,
                    json: false,
                };
                match aifo_coder::fork_clean(repo_root, &opts) {
                    Ok(_) => {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[32;1m",
                                &format!("aifo-coder: disposed fork session {}.", sid)
                            )
                        );
                    }
                    Err(e) => {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[33m",
                                &format!(
                                    "aifo-coder: warning: failed to dispose fork session {}: {}",
                                    sid, e
                                )
                            )
                        );
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            if plain {
                eprintln!(
                    "aifo-coder: merge strategy '{}' failed: {}",
                    strat, e
                );
            } else {
                let use_err = aifo_coder::color_enabled_stderr();
                eprintln!(
                    "{}",
                    aifo_coder::paint(
                        use_err,
                        "\x1b[31;1m",
                        &format!("aifo-coder: merge strategy '{}' failed: {}", strat, e)
                    )
                );
            }
            Err(e.to_string())
        }
    }
}
