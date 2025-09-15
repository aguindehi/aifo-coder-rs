//! Refuse/confirm prompts for fork clean: print summaries, handle non-interactive safeguards.
use std::io::Write;

use crate::fork::fork_impl_clean_plan::SessionPlan;
use crate::{color_enabled_stderr, paint};

/// Check protections and (when required) prompt interactively.
/// Returns Ok(()) to proceed; Err(exit_code) to abort or refuse.
pub fn check_and_prompt(plan: &[SessionPlan], opts: &crate::ForkCleanOpts) -> Result<(), i32> {
    // Default protection: if any protected pane and neither --force nor --keep-dirty, refuse
    if !opts.force && !opts.keep_dirty {
        let mut protected = 0usize;
        for sp in plan {
            for ps in &sp.panes {
                if !ps.clean {
                    protected += 1;
                }
            }
        }
        if protected > 0 {
            let use_err = color_enabled_stderr();
            eprintln!(
                "{}: {} pane(s) are protected (dirty/ahead/base-unknown).",
                paint(use_err, "\x1b[31;1m", "aifo-coder: refusing to delete"),
                protected
            );
            eprintln!(
                "{}",
                paint(
                    use_err,
                    "\x1b[33m",
                    "Use --keep-dirty to remove only clean panes, or --force to delete everything."
                )
            );
            // Print summary
            for sp in plan {
                let sid = sp
                    .dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(unknown)");
                for ps in &sp.panes {
                    if !ps.clean {
                        eprintln!(
                            "  {} :: {} [{}]",
                            paint(use_err, "\x1b[34;1m", sid),
                            ps.dir.display(),
                            paint(use_err, "\x1b[33m", &ps.reasons.join(","))
                        );
                    }
                }
            }
            return Err(1);
        }
    }

    // Interactive confirmation before deletion (safety prompt)
    if !opts.dry_run && !opts.yes && !opts.json {
        if !atty::is(atty::Stream::Stdin) {
            eprintln!("aifo-coder: refusing to delete without confirmation on non-interactive stdin. Re-run with --yes or --dry-run.");
            return Err(1);
        }
        let mut del_sessions = 0usize;
        let mut del_panes = 0usize;
        if opts.force {
            del_sessions = plan.len();
            for sp in plan {
                del_panes += sp.panes.len();
            }
        } else if opts.keep_dirty {
            for sp in plan {
                let clean_count = sp.panes.iter().filter(|ps| ps.clean).count();
                del_panes += clean_count;
                let remaining = sp.panes.len().saturating_sub(clean_count);
                if remaining == 0 {
                    del_sessions += 1;
                }
            }
        } else {
            del_sessions = plan.len();
            for sp in plan {
                del_panes += sp.panes.len();
            }
        }
        if del_sessions > 0 || del_panes > 0 {
            let prompt = format!(
                "aifo-coder: about to delete {} session(s) and {} pane(s). Proceed? [y/N] ",
                del_sessions, del_panes
            );
            let use_err = color_enabled_stderr();
            eprint!("{}", paint(use_err, "\x1b[33m", &prompt));
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            let ans = line.trim().to_ascii_lowercase();
            if ans != "y" && ans != "yes" {
                eprintln!("aborted.");
                return Err(1);
            }
        }
    }

    Ok(())
}
