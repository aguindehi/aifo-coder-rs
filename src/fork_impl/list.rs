use std::env;
use std::fs;
use std::path::Path;
use std::time::SystemTime;

pub(crate) fn fork_list_impl(
    repo_root: &Path,
    json: bool,
    all_repos: bool,
) -> std::io::Result<i32> {
    // Threshold for stale highlighting in list output (default 14d)
    let list_stale_days: u64 = env::var("AIFO_CODER_FORK_LIST_STALE_DAYS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(14);

    // Helper to collect rows for a single repo
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

    if all_repos {
        // Optional workspace scan when AIFO_CODER_WORKSPACE_ROOT is set
        if let Ok(ws) = env::var("AIFO_CODER_WORKSPACE_ROOT") {
            let ws_path = Path::new(&ws);
            if ws_path.is_dir() {
                let mut any = false;
                if json {
                    let mut out = String::from("[");
                    let mut first = true;
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
                                if !first {
                                    out.push(',');
                                }
                                first = false;
                                out.push_str(&format!(
                                    "{{\"repo_root\":{},\"sid\":\"{}\",\"panes\":{},\"created_at\":{},\"age_days\":{},\"base_label\":{},\"stale\":{}}}",
                                    crate::json_escape(&repo.display().to_string()),
                                    sid,
                                    panes,
                                    created_at,
                                    age_days,
                                    crate::json_escape(&base_label),
                                    if stale { "true" } else { "false" }
                                ));
                            }
                        }
                    }
                    out.push(']');
                    println!("{}", out);
                } else {
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
                            println!(
                                "{} {}",
                                crate::paint(
                                    use_color,
                                    "\x1b[36;1m",
                                    "aifo-coder: fork sessions under"
                                ),
                                crate::paint(use_color, "\x1b[34;1m", &header_path)
                            );
                            for (sid, panes, _created_at, age_days, base_label, stale) in rows {
                                let base_col = crate::paint(use_color, "\x1b[34;1m", &base_label);
                                if stale {
                                    let stale_col = crate::paint(use_color, "\x1b[33m", "(stale)");
                                    println!(
                                        "  {}  panes={}  age={}d  base={}  {}",
                                        sid, panes, age_days, base_col, stale_col
                                    );
                                } else {
                                    println!(
                                        "  {}  panes={}  age={}d  base={}",
                                        sid, panes, age_days, base_col
                                    );
                                }
                            }
                        }
                    }
                    if !any {
                        println!(
                            "aifo-coder: no fork sessions found under workspace {}",
                            ws_path.display()
                        );
                    }
                }
                return Ok(0);
            }
            // If workspace root is invalid, report error when --all-repos was requested
            eprintln!("aifo-coder: --all-repos requires AIFO_CODER_WORKSPACE_ROOT to be set to an existing directory.");
            return Ok(1);
        } else {
            // Missing env var: explicitly error when --all-repos is requested without workspace root
            eprintln!("aifo-coder: --all-repos requires AIFO_CODER_WORKSPACE_ROOT to be set to an existing directory.");
            return Ok(1);
        }
    }

    // Single repository case (default)
    let rows = collect_rows(repo_root, list_stale_days);
    if rows.is_empty() {
        if json {
            println!("[]");
        } else {
            let base = repo_root.join(".aifo-coder").join("forks");
            println!(
                "aifo-coder: no fork sessions found under {}",
                base.display()
            );
        }
        return Ok(0);
    }

    if json {
        let mut out = String::from("[");
        for (idx, (sid, panes, created_at, age_days, base_label, stale)) in rows.iter().enumerate()
        {
            if idx > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"repo_root\":{},\"sid\":\"{}\",\"panes\":{},\"created_at\":{},\"age_days\":{},\"base_label\":{},\"stale\":{}}}",
                crate::json_escape(&repo_root.display().to_string()),
                sid, panes, created_at, age_days, crate::json_escape(base_label), if *stale { "true" } else { "false" }
            ));
        }
        out.push(']');
        println!("{}", out);
    } else {
        let use_color = crate::color_enabled_stdout();
        let header_path = format!("{}/.aifo-coder/forks", repo_root.display());
        println!(
            "{} {}",
            crate::paint(use_color, "\x1b[36;1m", "aifo-coder: fork sessions under"),
            crate::paint(use_color, "\x1b[34;1m", &header_path)
        );
        for (sid, panes, _created_at, age_days, base_label, stale) in rows {
            let base_col = crate::paint(use_color, "\x1b[34;1m", &base_label);
            if stale {
                let stale_col = crate::paint(use_color, "\x1b[33m", "(stale)");
                println!(
                    "  {}  panes={}  age={}d  base={}  {}",
                    sid, panes, age_days, base_col, stale_col
                );
            } else {
                println!(
                    "  {}  panes={}  age={}d  base={}",
                    sid, panes, age_days, base_col
                );
            }
        }
    }
    Ok(0)
}
