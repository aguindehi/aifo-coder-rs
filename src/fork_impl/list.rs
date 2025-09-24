//! Fork list collection and rendering helpers (preserve exact strings and ordering).
use std::env;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Collect list rows for a single repository (internal helper).
fn collect_rows(
    repo_root: &Path,
    list_stale_days: u64,
) -> Vec<(String, usize, u64, u64, String, bool)> {
    let mut rows = Vec::new();
    let base = repo_root.join(".aifo-coder").join("forks");
    if !base.exists() {
        return rows;
    }
    for sd in super::fork_impl_scan::session_dirs(&base) {
        let sid = sd
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if sid.is_empty() {
            continue;
        }
        let meta_path = sd.join(".meta.json");
        let meta = fs::read_to_string(&meta_path).ok();
        let created_at = meta
            .as_deref()
            .and_then(|s| crate::fork_meta::extract_value_u64(s, "created_at"))
            .or_else(|| {
                fs::metadata(&sd)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .map(super::fork_impl_scan::secs_since_epoch)
            })
            .unwrap_or(0);
        let base_label = meta
            .as_deref()
            .and_then(|s| crate::fork_meta::extract_value_string(s, "base_label"))
            .unwrap_or_else(|| "(unknown)".to_string());
        let panes = super::fork_impl_scan::pane_dirs_for_session(&sd).len();
        let now = super::fork_impl_scan::secs_since_epoch(SystemTime::now());
        let age_days = if created_at > 0 {
            now.saturating_sub(created_at) / 86400
        } else {
            0
        };
        let stale = (age_days as u64) >= list_stale_days;
        rows.push((sid, panes, created_at, age_days, base_label, stale));
    }
    rows.sort_by_key(|r| r.2);
    rows
}

/// Format a single JSON row exactly as fork_list_impl emits it.
fn format_row_json(
    repo_root: &Path,
    sid: &str,
    panes: usize,
    created_at: u64,
    age_days: u64,
    base_label: &str,
    stale: bool,
) -> String {
    format!(
        "{{\"repo_root\":{},\"sid\":\"{}\",\"panes\":{},\"created_at\":{},\"age_days\":{},\"base_label\":{},\"stale\":{}}}",
        crate::json_escape(&repo_root.display().to_string()),
        sid,
        panes,
        created_at,
        age_days,
        crate::json_escape(base_label),
        if stale { "true" } else { "false" }
    )
}

pub(crate) fn fork_list_to_string_impl(
    repo_root: &Path,
    json: bool,
    all_repos: bool,
) -> std::io::Result<String> {
    // Threshold for stale highlighting in list output (default 14d)
    let list_stale_days: u64 = env::var("AIFO_CODER_FORK_LIST_STALE_DAYS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(14);

    if all_repos {
        // Workspace scan requires AIFO_CODER_WORKSPACE_ROOT to be set to an existing directory
        let ws = env::var("AIFO_CODER_WORKSPACE_ROOT").map_err(|_| {
            std::io::Error::other(crate::display_for_fork_error(&crate::ForkError::Message(
                "workspace root env missing".to_string(),
            )))
        })?;
        let ws_path = Path::new(&ws);
        if !ws_path.is_dir() {
            return Err(std::io::Error::other(crate::display_for_fork_error(
                &crate::ForkError::Message(
                    "--all-repos requires AIFO_CODER_WORKSPACE_ROOT to be set to an existing directory."
                        .to_string(),
                ),
            )));
        }

        if json {
            let mut items: Vec<String> = Vec::new();
            if let Ok(rd) = fs::read_dir(ws_path) {
                for ent in rd.flatten() {
                    let repo = ent.path();
                    if !repo.is_dir() {
                        continue;
                    }
                    let forks_dir = repo.join(".aifo-coder").join("forks");
                    if !forks_dir.exists() {
                        continue;
                    }
                    let rows = collect_rows(&repo, list_stale_days);
                    for (sid, panes, created_at, age_days, base_label, stale) in rows {
                        items.push(format_row_json(
                            &repo,
                            &sid,
                            panes,
                            created_at,
                            age_days,
                            &base_label,
                            stale,
                        ));
                    }
                }
            }
            let out = format!("[{}]", items.join(","));
            return Ok(out);
        } else {
            let mut blocks: Vec<String> = Vec::new();
            let mut any = false;
            if let Ok(rd) = fs::read_dir(ws_path) {
                for ent in rd.flatten() {
                    let repo = ent.path();
                    if !repo.is_dir() {
                        continue;
                    }
                    let forks_dir = repo.join(".aifo-coder").join("forks");
                    if !forks_dir.exists() {
                        continue;
                    }
                    let rows = collect_rows(&repo, list_stale_days);
                    if rows.is_empty() {
                        continue;
                    }
                    any = true;
                    let use_color = crate::color_enabled_stdout();
                    let header_path = format!("{}/.aifo-coder/forks", repo.display());
                    let mut lines: Vec<String> = Vec::new();
                    lines.push(format!(
                        "{} {}",
                        crate::paint(use_color, "\x1b[36;1m", "aifo-coder: fork sessions under"),
                        crate::paint(use_color, "\x1b[34;1m", &header_path)
                    ));
                    for (sid, panes, _created_at, age_days, base_label, stale) in rows {
                        let base_text = if use_color {
                            crate::paint(use_color, "\x1b[34;1m", &base_label)
                        } else {
                            base_label.clone()
                        };
                        if stale {
                            let stale_text = if use_color {
                                crate::paint(use_color, "\x1b[33m", "(stale)")
                            } else {
                                "(stale)".to_string()
                            };
                            lines.push(format!(
                                "  {}  panes={}  age={}d  base={}  {}",
                                sid, panes, age_days, base_text, stale_text
                            ));
                        } else {
                            lines.push(format!(
                                "  {}  panes={}  age={}d  base={}",
                                sid, panes, age_days, base_text
                            ));
                        }
                    }
                    blocks.push(lines.join("\n"));
                }
            }
            if !any {
                return Ok(format!(
                    "aifo-coder: no fork sessions found under workspace {}",
                    ws_path.display()
                ));
            }
            return Ok(blocks.join("\n"));
        }
    }

    // Single repository case (default)
    let rows = collect_rows(repo_root, list_stale_days);
    if rows.is_empty() {
        if json {
            return Ok("[]".to_string());
        } else {
            let base = repo_root.join(".aifo-coder").join("forks");
            return Ok(format!(
                "aifo-coder: no fork sessions found under {}",
                base.display()
            ));
        }
    }

    if json {
        let mut items: Vec<String> = Vec::new();
        for (sid, panes, created_at, age_days, base_label, stale) in rows {
            items.push(format_row_json(
                repo_root,
                &sid,
                panes,
                created_at,
                age_days,
                &base_label,
                stale,
            ));
        }
        Ok(format!("[{}]", items.join(",")))
    } else {
        let use_color = crate::color_enabled_stdout();
        let header_path = format!("{}/.aifo-coder/forks", repo_root.display());
        let mut lines: Vec<String> = Vec::new();
        lines.push(format!(
            "{} {}",
            crate::paint(use_color, "\x1b[36;1m", "aifo-coder: fork sessions under"),
            crate::paint(use_color, "\x1b[34;1m", &header_path)
        ));
        for (sid, panes, _created_at, age_days, base_label, stale) in rows {
            let base_text = if use_color {
                crate::paint(use_color, "\x1b[34;1m", &base_label)
            } else {
                base_label.clone()
            };
            if stale {
                let stale_text = if use_color {
                    crate::paint(use_color, "\x1b[33m", "(stale)")
                } else {
                    "(stale)".to_string()
                };
                lines.push(format!(
                    "  {}  panes={}  age={}d  base={}  {}",
                    sid, panes, age_days, base_text, stale_text
                ));
            } else {
                lines.push(format!(
                    "  {}  panes={}  age={}d  base={}",
                    sid, panes, age_days, base_text
                ));
            }
        }
        Ok(lines.join("\n"))
    }
}

pub(crate) fn fork_list_impl(
    repo_root: &Path,
    json: bool,
    all_repos: bool,
) -> std::io::Result<i32> {
    match fork_list_to_string_impl(repo_root, json, all_repos) {
        Ok(s) => {
            println!("{}", s);
            Ok(0)
        }
        Err(_) => {
            eprintln!("aifo-coder: --all-repos requires AIFO_CODER_WORKSPACE_ROOT to be set to an existing directory.");
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_collect_rows_order_and_stale_flag() {
        let td = tempdir().expect("tmpdir");
        let repo = td.path();
        let forks = repo.join(".aifo-coder").join("forks");
        fs::create_dir_all(&forks).unwrap();

        let now = super::super::fork_impl_scan::secs_since_epoch(SystemTime::now());

        // Session 1: older (stale)
        let s1 = forks.join("sid-old");
        fs::create_dir_all(&s1).unwrap();
        fs::create_dir_all(s1.join("pane-1")).unwrap();
        let meta1 = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\" }}",
            now.saturating_sub(10 * 86400)
        );
        fs::write(s1.join(".meta.json"), meta1).unwrap();

        // Session 2: recent (not stale)
        let s2 = forks.join("sid-new");
        fs::create_dir_all(&s2).unwrap();
        fs::create_dir_all(s2.join("pane-1")).unwrap();
        let meta2 = format!(
            "{{ \"created_at\": {}, \"base_label\": \"dev\" }}",
            now.saturating_sub(2 * 86400)
        );
        fs::write(s2.join(".meta.json"), meta2).unwrap();

        let rows = collect_rows(repo, 5);
        assert_eq!(rows.len(), 2, "expected two sessions");
        // Sorted by created_at ascending: older first
        assert_eq!(rows[0].0, "sid-old", "older session should come first");
        assert!(rows[0].5, "sid-old should be stale (>=5d)");
        assert!(!rows[1].5, "sid-new should not be stale (<5d)");
    }

    #[test]
    fn test_format_row_json_golden_single_repo() {
        let td = tempdir().expect("tmpdir");
        let repo = td.path();
        let sid = "sid-1";
        let panes = 3usize;
        let created_at = 123456789u64;
        let age_days = 42u64;
        let base_label = "main";
        let stale = true;

        let actual =
            super::format_row_json(repo, sid, panes, created_at, age_days, base_label, stale);
        let expected = format!(
            "{{\"repo_root\":{},\"sid\":\"{}\",\"panes\":{},\"created_at\":{},\"age_days\":{},\"base_label\":{},\"stale\":true}}",
            crate::json_escape(&repo.display().to_string()),
            sid,
            panes,
            created_at,
            age_days,
            crate::json_escape(base_label)
        );
        assert_eq!(actual, expected, "JSON row format must match exactly");
    }
}
