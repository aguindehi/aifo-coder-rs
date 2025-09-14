 //! Execute deletion for fork clean: delete sessions/panes, update metadata; supports dry-run previews.
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::fork::fork_impl_clean_plan::SessionPlan;
use crate::{color_enabled_stdout, json_escape, paint, toolchain_cleanup_session};

/// Execute deletions (or print in dry-run); returns (deleted_sessions_count, deleted_panes_count).
pub fn execute(
    plan: &[SessionPlan],
    opts: &crate::ForkCleanOpts,
) -> std::io::Result<(usize, usize)> {
    let mut deleted_sessions_count: usize = 0;
    let mut deleted_panes_count: usize = 0;

    for sp in plan {
        let sd = &sp.dir;
        let panes = &sp.panes;
        let sid = sd
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unknown)")
            .to_string();
        if opts.force {
            if opts.dry_run {
                let use_out = color_enabled_stdout();
                println!(
                    "{} {}",
                    paint(use_out, "\x1b[33m", "DRY-RUN:"),
                    paint(use_out, "\x1b[34;1m", &format!("rm -rf {}", sd.display()))
                );
            } else {
                deleted_panes_count += panes.len();
                deleted_sessions_count += 1;
                toolchain_cleanup_session(&sid, false);
                let _ = fs::remove_dir_all(sd);
                let use_out = color_enabled_stdout();
                println!(
                    "{}",
                    paint(
                        use_out,
                        "\x1b[32;1m",
                        &format!("aifo-coder: deleted fork session {}", sid)
                    )
                );
            }
            continue;
        }
        if opts.keep_dirty {
            let mut remaining: Vec<PathBuf> = Vec::new();
            for ps in panes {
                if ps.clean {
                    if opts.dry_run {
                        let use_out = color_enabled_stdout();
                        println!(
                            "{} {}",
                            paint(use_out, "\x1b[33m", "DRY-RUN:"),
                            paint(
                                use_out,
                                "\x1b[34;1m",
                                &format!("rm -rf {}", ps.dir.display())
                            )
                        );
                    } else {
                        deleted_panes_count += 1;
                        let _ = fs::remove_dir_all(&ps.dir);
                    }
                } else {
                    remaining.push(ps.dir.clone());
                }
            }
            if remaining.is_empty() {
                if opts.dry_run {
                    let use_out = color_enabled_stdout();
                    println!(
                        "{} {}",
                        paint(use_out, "\x1b[33m", "DRY-RUN:"),
                        paint(use_out, "\x1b[34;1m", &format!("rmdir {}", sd.display()))
                    );
                } else {
                    deleted_sessions_count += 1;
                    toolchain_cleanup_session(&sid, false);
                    let _ = fs::remove_dir_all(sd);
                    let use_out = color_enabled_stdout();
                    println!(
                        "{}",
                        paint(
                            use_out,
                            "\x1b[32;1m",
                            &format!("aifo-coder: deleted fork session {}", sid)
                        )
                    );
                }
            } else {
                // Update .meta.json with remaining panes (also refresh branches best-effort)
                if !opts.dry_run {
                    let mut branches: Vec<String> = Vec::new();
                    for p in &remaining {
                        if let Ok(out) = {
                            let mut cmd = aifo_coder::fork_impl_git::git_cmd(Some(p));
                            cmd.arg("rev-parse")
                                .arg("--abbrev-ref")
                                .arg("HEAD")
                                .stdout(Stdio::piped())
                                .stderr(Stdio::null());
                            cmd.output()
                        } {
                            if out.status.success() {
                                let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                if !b.is_empty() {
                                    branches.push(b);
                                }
                            }
                        }
                    }

                    // Enrich metadata with prior fields and use valid JSON escaping
                    let prev = fs::read_to_string(sd.join(".meta.json")).ok();
                    let created_at_num = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_u64(s, "created_at"))
                        .unwrap_or(0);
                    let base_label_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_label"))
                        .unwrap_or_else(|| "(unknown)".to_string());
                    let base_ref_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_ref_or_sha"))
                        .unwrap_or_default();
                    let base_commit_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_commit_sha"))
                        .unwrap_or_default();
                    let layout_prev = prev
                        .as_deref()
                        .and_then(|s| crate::fork_meta::extract_value_string(s, "layout"))
                        .unwrap_or_else(|| "tiled".to_string());

                    let mut meta_out = String::from("{");
                    meta_out.push_str(&format!("\"sid\":{},", json_escape(&sid)));
                    meta_out.push_str(&format!("\"created_at\":{},", created_at_num));
                    meta_out.push_str(&format!(
                        "\"base_label\":{},",
                        json_escape(&base_label_prev)
                    ));
                    meta_out.push_str(&format!(
                        "\"base_ref_or_sha\":{},",
                        json_escape(&base_ref_prev)
                    ));
                    meta_out.push_str(&format!(
                        "\"base_commit_sha\":{},",
                        json_escape(&base_commit_prev)
                    ));
                    meta_out.push_str(&format!("\"layout\":{},", json_escape(&layout_prev)));
                    meta_out.push_str(&format!("\"panes_remaining\":{},", remaining.len()));
                    meta_out.push_str("\"pane_dirs\":[");
                    for (idx, p) in remaining.iter().enumerate() {
                        if idx > 0 {
                            meta_out.push(',');
                        }
                        meta_out.push_str(&json_escape(&p.display().to_string()));
                    }
                    meta_out.push_str("],\"branches\":[");
                    for (i, b) in branches.iter().enumerate() {
                        if i > 0 {
                            meta_out.push(',');
                        }
                        meta_out.push_str(&json_escape(b));
                    }
                    meta_out.push_str("]}");
                    let _ = fs::write(sd.join(".meta.json"), meta_out);
                    let use_out = color_enabled_stdout();
                    println!(
                        "{}",
                        paint(
                            use_out,
                            "\x1b[33m",
                            &format!(
                                "aifo-coder: kept fork session {} ({} protected pane(s) remain)",
                                sid,
                                remaining.len()
                            )
                        )
                    );
                }
            }
        } else {
            // all panes are clean here (or we would have bailed above)
            if opts.dry_run {
                let use_out = color_enabled_stdout();
                println!(
                    "{} {}",
                    paint(use_out, "\x1b[33m", "DRY-RUN:"),
                    paint(use_out, "\x1b[34;1m", &format!("rm -rf {}", sd.display()))
                );
            } else {
                deleted_panes_count += panes.len();
                deleted_sessions_count += 1;
                toolchain_cleanup_session(&sid, false);
                let _ = fs::remove_dir_all(sd);
                let use_out = color_enabled_stdout();
                println!(
                    "{}",
                    paint(
                        use_out,
                        "\x1b[32;1m",
                        &format!("aifo-coder: deleted fork session {}", sid)
                    )
                );
            }
        }
    }

    Ok((deleted_sessions_count, deleted_panes_count))
}
