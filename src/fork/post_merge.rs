use std::path::Path;

/// Apply post-merge with standardized logging. When `plain` is true, print
/// non-colored success/failure and octopus-start messages; the initial "applying"
/// line remains color-aware to match existing behavior.
pub fn apply_post_merge(
    repo_root: &Path,
    sid: &str,
    strategy: crate::MergingStrategy,
    autoclean: bool,
    dry_run: bool,
    verbose: bool,
    plain: bool,
) -> Result<(), String> {
    if matches!(strategy, crate::MergingStrategy::None) {
        return Ok(());
    }
    let strat = match strategy {
        crate::MergingStrategy::None => "none",
        crate::MergingStrategy::Fetch => "fetch",
        crate::MergingStrategy::Octopus => "octopus",
    };
    {
        let use_err = crate::color_enabled_stderr();
        crate::log_info_stderr(
            use_err,
            &format!("aifo-coder: applying post-fork merge strategy: {}", strat),
        );
    }
    match crate::fork_merge_branches_by_session(repo_root, sid, strategy, verbose, dry_run) {
        Ok(()) => {
            if plain {
                eprintln!("aifo-coder: merge strategy '{}' completed.", strat);
            } else {
                let use_err = crate::color_enabled_stderr();
                eprintln!(
                    "{}",
                    crate::paint(
                        use_err,
                        "\x1b[32;1m",
                        &format!("aifo-coder: merge strategy '{}' completed.", strat)
                    )
                );
            }
            if matches!(strategy, crate::MergingStrategy::Octopus) && autoclean && !dry_run {
                eprintln!();
                if plain {
                    eprintln!(
                        "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                        sid
                    );
                } else {
                    let use_err = crate::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        crate::paint(
                            use_err,
                            "\x1b[36;1m",
                            &format!(
                                "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                sid
                            )
                        )
                    );
                }
                let opts = crate::ForkCleanOpts {
                    session: Some(sid.to_string()),
                    older_than_days: None,
                    all: false,
                    dry_run: false,
                    yes: true,
                    force: true,
                    keep_dirty: false,
                    json: false,
                };
                match crate::fork_clean(repo_root, &opts) {
                    Ok(_) => {
                        let use_err = crate::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            crate::paint(
                                use_err,
                                "\x1b[32;1m",
                                &format!("aifo-coder: disposed fork session {}.", sid)
                            )
                        );
                    }
                    Err(e) => {
                        let use_err = crate::color_enabled_stderr();
                        crate::log_warn_stderr(
                            use_err,
                            &format!(
                                "aifo-coder: warning: failed to dispose fork session {}: {}",
                                sid, e
                            ),
                        );
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            if plain {
                eprintln!("aifo-coder: merge strategy '{}' failed: {}", strat, e);
            } else {
                let use_err = crate::color_enabled_stderr();
                crate::log_error_stderr(
                    use_err,
                    &format!("aifo-coder: merge strategy '{}' failed: {}", strat, e),
                );
            }
            Err(e.to_string())
        }
    }
}
