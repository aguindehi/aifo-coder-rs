#![allow(clippy::module_name_repetitions)]
//! Fork lifecycle: repo detection, snapshotting, cloning panes, merging, cleaning and notices.

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use crate::{
    color_enabled_stderr, color_enabled_stdout, json_escape, paint, shell_join,
    toolchain_cleanup_session,
};

#[path = "fork_impl/scan.rs"]
mod fork_impl_scan;
#[path = "fork_impl/git.rs"]
mod fork_impl_git;
#[path = "fork_impl/panecheck.rs"]
mod fork_impl_panecheck;
#[path = "fork_impl/notice.rs"]
mod fork_impl_notice;

/// Try to detect the Git repository root (absolute canonical path).
/// Returns Some(repo_root) when inside a Git repository; otherwise None.
pub fn repo_root() -> Option<PathBuf> {
    // Use helper to detect the repository top-level
    let s = match fork_impl_git::git_stdout_str(None, &["rev-parse", "--show-toplevel"]) {
        Some(v) => v.trim().to_string(),
        None => return None,
    };
    if s.is_empty() {
        return None;
    }
    let p = PathBuf::from(s);
    // Prefer canonical absolute path if possible
    fs::canonicalize(&p).ok().or(Some(p))
}

// Sanitize a ref path component: lowercase, replace invalid chars with '-',
// collapse repeated '-', and strip leading/trailing '/', '-' and '.'.
// Additionally trim to a safe length to keep branch names manageable.
pub fn fork_sanitize_base_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = false;
    for ch in s.chars().map(|c| c.to_ascii_lowercase()) {
        let valid = ch.is_ascii_alphanumeric();
        let replace = match ch {
            '-' | '.' | '/' | ' ' | '_' => true, // treat these as separators
            _ => !valid,
        };
        if replace {
            if !last_dash && !out.is_empty() {
                out.push('-');
                last_dash = true;
            }
        } else {
            out.push(ch);
            last_dash = false;
        }
    }
    // Trim leading/trailing separators
    while matches!(out.chars().next(), Some('-') | Some('/') | Some('.')) {
        out.remove(0);
    }
    while matches!(out.chars().last(), Some('-') | Some('/') | Some('.')) {
        out.pop();
    }
    let mut res = if out.is_empty() {
        "base".to_string()
    } else {
        out
    };
    // Collapse any accidental double dashes that may remain
    while res.contains("--") {
        res = res.replace("--", "-");
    }
    // Enforce a conservative max length for the component
    const MAX_LEN: usize = 48;
    if res.len() > MAX_LEN {
        res.truncate(MAX_LEN);
        // Avoid trailing dash after truncation
        while matches!(res.chars().last(), Some('-') | Some('/') | Some('.')) && !res.is_empty() {
            res.pop();
        }
        if res.is_empty() {
            res = "base".to_string();
        }
    }
    res
}

/// Compute base ref/SHA and label for the current repository state.
/// Returns (base_label, base_ref_or_sha, base_commit_sha).
pub fn fork_base_info(repo_root: &Path) -> std::io::Result<(String, String, String)> {
    let root = repo_root;
    // Determine current branch or detached state
    let branch_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let head_out = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--verify")
        .arg("HEAD")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();

    let head_sha = head_out
        .as_ref()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let (base_label, base_ref_or_sha) = if branch_out.status.success() {
        let name = String::from_utf8_lossy(&branch_out.stdout)
            .trim()
            .to_string();
        if name == "HEAD" {
            ("detached".to_string(), head_sha.clone())
        } else {
            (fork_sanitize_base_label(&name), name)
        }
    } else {
        ("detached".to_string(), head_sha.clone())
    };
    Ok((base_label, base_ref_or_sha, head_sha))
}

/// Create a temporary snapshot commit that includes staged + unstaged changes without
/// altering user index or working tree. Uses a temporary index (GIT_INDEX_FILE) and
/// git commit-tree. Returns the new snapshot commit SHA on success.
pub fn fork_create_snapshot(repo_root: &Path, sid: &str) -> std::io::Result<String> {
    // Create a unique temporary index path (under .git when possible)
    let tmp_idx = {
        let git_dir_out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("rev-parse")
            .arg("--git-dir")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok();
        let git_dir = git_dir_out
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .map(PathBuf::from)
            .unwrap_or_else(|| repo_root.join(".git"));
        let pid = std::process::id();
        let idx_name = format!("index.aifo-{}-{}", sid, pid);
        git_dir.join(idx_name)
    };
    // Helper to run git with the temporary index
    let with_tmp_index = |args: &[&str]| -> std::io::Result<std::process::Output> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(repo_root);
        for a in args {
            cmd.arg(a);
        }
        cmd.env("GIT_INDEX_FILE", &tmp_idx);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        cmd.output()
    };

    // 1) Index current working tree: git add -A
    let add_out = with_tmp_index(&["add", "-A"])?;
    if !add_out.status.success() {
        let _ = fs::remove_file(&tmp_idx);
        return Err(std::io::Error::other("git add -A failed for snapshot"));
    }

    // 2) write-tree
    let wt = with_tmp_index(&["write-tree"])?;
    if !wt.status.success() {
        let _ = fs::remove_file(&tmp_idx);
        return Err(std::io::Error::other("git write-tree failed for snapshot"));
    }
    let tree = String::from_utf8_lossy(&wt.stdout).trim().to_string();

    // 3) Determine parent if any (HEAD may not exist)
    let parent = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("rev-parse")
        .arg("--verify")
        .arg("HEAD")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    // 4) commit-tree
    let mut ct = Command::new("git");
    ct.arg("-C").arg(repo_root);
    ct.arg("commit-tree").arg(&tree);
    if let Some(p) = parent.as_deref() {
        ct.arg("-p").arg(p);
    }
    ct.arg("-m").arg(format!("aifo-fork snapshot {}", sid));
    ct.stdout(Stdio::piped()).stderr(Stdio::piped());
    let ct_out = ct.output()?;
    // Clean up temporary index (best-effort)
    let _ = fs::remove_file(&tmp_idx);
    if !ct_out.status.success() {
        return Err(std::io::Error::other(format!(
            "git commit-tree failed for snapshot: {}",
            String::from_utf8_lossy(&ct_out.stderr)
        )));
    }
    let sha = String::from_utf8_lossy(&ct_out.stdout).trim().to_string();
    if sha.is_empty() {
        return Err(std::io::Error::other("empty snapshot SHA from commit-tree"));
    }
    Ok(sha)
}

/// Construct the fork branch name for pane i (1-based): fork/<base-label>/<sid>-<i>
pub fn fork_branch_name(base_label: &str, sid: &str, i: usize) -> String {
    format!(
        "fork/{}/{}-{}",
        fork_sanitize_base_label(base_label),
        sid,
        i
    )
}

/// Base directory for fork panes: <repo-root>/.aifo-coder/forks/<sid>
pub fn fork_session_dir(repo_root: &Path, sid: &str) -> PathBuf {
    repo_root.join(".aifo-coder").join("forks").join(sid)
}

/// Quick heuristic to detect if a repository uses Git LFS without requiring git-lfs to be installed.
/// Returns true if:
/// - .lfsconfig exists at repo root, or
/// - any .gitattributes file (top-level or nested) contains "filter=lfs".
pub fn repo_uses_lfs_quick(repo_root: &Path) -> bool {
    // .lfsconfig presence is a strong hint
    if repo_root.join(".lfsconfig").exists() {
        return true;
    }
    // Top-level .gitattributes
    if let Ok(s) = fs::read_to_string(repo_root.join(".gitattributes")) {
        if s.contains("filter=lfs") {
            return true;
        }
    }
    // Scan nested .gitattributes files (skip .git directory)
    fn scan(dir: &Path) -> bool {
        let rd = match fs::read_dir(dir) {
            Ok(d) => d,
            Err(_) => return false,
        };
        for ent in rd {
            let Ok(ent) = ent else { continue };
            let path = ent.path();
            let Ok(ft) = ent.file_type() else { continue };
            if ft.is_dir() {
                // Skip VCS directory
                if ent.file_name().to_string_lossy() == ".git" {
                    continue;
                }
                if scan(&path) {
                    return true;
                }
            } else if ft.is_file() && ent.file_name().to_string_lossy() == ".gitattributes" {
                if let Ok(s) = fs::read_to_string(&path) {
                    if s.contains("filter=lfs") {
                        return true;
                    }
                }
            }
        }
        false
    }
    scan(repo_root)
}

/// Clone and checkout N fork panes based on a base ref/SHA.
/// Each pane is created under <repo-root>/.aifo-coder/forks/<sid>/pane-<i> and
/// on success returns a vector of (pane_dir, branch_name).
pub fn fork_clone_and_checkout_panes(
    repo_root: &Path,
    sid: &str,
    panes: usize,
    base_ref_or_sha: &str,
    base_label: &str,
    dissociate: bool,
) -> std::io::Result<Vec<(PathBuf, String)>> {
    if panes < 1 {
        return Ok(Vec::new());
    }
    let repo_abs = fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let root_str = repo_abs.to_string_lossy().to_string();
    let src_url = if cfg!(windows) {
        format!("file:///{}", root_str.replace('\\', "/"))
    } else {
        format!("file://{}", root_str)
    };
    let session_dir = fork_session_dir(&repo_abs, sid);
    fs::create_dir_all(&session_dir)?;

    // Try to capture push URL from base repo (non-fatal if unavailable)
    let base_push_url = Command::new("git")
        .arg("-C")
        .arg(&repo_abs)
        .arg("remote")
        .arg("get-url")
        .arg("--push")
        .arg("origin")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let mut results: Vec<(PathBuf, String)> = Vec::with_capacity(panes);

    for i in 1..=panes {
        let pane_dir = session_dir.join(format!("pane-{}", i));
        // Try cloning using a plain local path first (most compatible), then fall back to file:// with protocol allow.
        let mut cloned_ok = false;
        for (source, allow_file_proto) in [(&root_str, false), (&src_url, true)] {
            let mut clone = Command::new("git");
            if allow_file_proto {
                // Newer Git may restrict file:// by default; allow it explicitly for local cloning.
                clone.arg("-c").arg("protocol.file.allow=always");
            }
            clone
                .arg("clone")
                .arg("--no-checkout")
                .arg("--reference-if-able")
                .arg(&root_str);
            if dissociate {
                clone.arg("--dissociate");
            }
            // repository URL/path and destination directory
            clone.arg(source).arg(&pane_dir);
            clone.stdout(Stdio::null()).stderr(Stdio::null());
            let st = clone.status()?;
            if st.success() {
                cloned_ok = true;
                break;
            } else {
                // Clean up any partial directory before next attempt
                let _ = fs::remove_dir_all(&pane_dir);
            }
        }
        if !cloned_ok {
            return Err(std::io::Error::other(format!(
                "git clone failed for pane {}",
                i
            )));
        }

        // Optional: set origin push URL to match base repo
        if let Some(ref url) = base_push_url {
            let _ = Command::new("git")
                .arg("-C")
                .arg(&pane_dir)
                .arg("remote")
                .arg("set-url")
                .arg("origin")
                .arg(url)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        // git checkout -b fork/<base>/<sid>-<i> <base_ref_or_sha>
        let branch = fork_branch_name(base_label, sid, i);
        let st = Command::new("git")
            .arg("-C")
            .arg(&pane_dir)
            .arg("checkout")
            .arg("-b")
            .arg(&branch)
            .arg(base_ref_or_sha)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !st.success() {
            let _ = fs::remove_dir_all(&pane_dir);
            return Err(std::io::Error::other(format!(
                "git checkout failed for pane {} (branch {})",
                i, branch
            )));
        }

        // Best-effort submodules and Git LFS
        if pane_dir.join(".gitmodules").exists() {
            let _ = Command::new("git")
                .arg("-c")
                .arg("protocol.file.allow=always")
                .arg("-C")
                .arg(&pane_dir)
                .arg("submodule")
                .arg("update")
                .arg("--init")
                .arg("--recursive")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
        // Git LFS: if git lfs is available and repository appears to use LFS, perform install/fetch/checkout
        let lfs_available = fork_impl_git::git_supports_lfs();
        if lfs_available {
            let uses_lfs = repo_uses_lfs_quick(&pane_dir);
            if uses_lfs {
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&pane_dir)
                    .arg("lfs")
                    .arg("install")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&pane_dir)
                    .arg("lfs")
                    .arg("fetch")
                    .arg("--all")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = Command::new("git")
                    .arg("-C")
                    .arg(&pane_dir)
                    .arg("lfs")
                    .arg("checkout")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }

        results.push((pane_dir, branch));
    }

    Ok(results)
}

/// Options for fork clean command.
pub struct ForkCleanOpts {
    pub session: Option<String>,
    pub older_than_days: Option<u64>,
    pub all: bool,
    pub dry_run: bool,
    pub yes: bool,
    pub force: bool,
    pub keep_dirty: bool,
    pub json: bool,
}

fn read_file_to_string(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok()
}

/* moved: use crate::fork_meta::extract_value_string / extract_value_u64 */

// Append or upsert merge metadata fields into the session .meta.json
/* moved: use crate::fork_meta::append_fields_compact */

/* moved: use fork_impl_scan::session_dirs */

/* moved: use fork_impl_scan::pane_dirs_for_session */

/* moved: use fork_impl_scan::secs_since_epoch */

/// List fork sessions under the current repository.
/// Returns exit code (0 on success).
pub fn fork_list(repo_root: &Path, json: bool, all_repos: bool) -> std::io::Result<i32> {
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
        for sd in fork_impl_scan::session_dirs(&base) {
            let sid = sd
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if sid.is_empty() {
                continue;
            }
            let meta_path = sd.join(".meta.json");
            let meta = read_file_to_string(&meta_path);
            let created_at = meta
                .as_deref()
                .and_then(|s| crate::fork_meta::extract_value_u64(s, "created_at"))
                .or_else(|| {
                    fs::metadata(&sd)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(fork_impl_scan::secs_since_epoch)
                })
                .unwrap_or(0);
            let base_label = meta
                .as_deref()
                .and_then(|s| crate::fork_meta::extract_value_string(s, "base_label"))
                .unwrap_or_else(|| "(unknown)".to_string());
            let panes = fork_impl_scan::pane_dirs_for_session(&sd).len();
            let now = fork_impl_scan::secs_since_epoch(SystemTime::now());
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
                                    json_escape(&repo.display().to_string()),
                                    sid,
                                    panes,
                                    created_at,
                                    age_days,
                                    json_escape(&base_label),
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
                            let use_color = color_enabled_stdout();
                            let header_path = format!("{}/.aifo-coder/forks", repo.display());
                            println!(
                                "{} {}",
                                paint(use_color, "\x1b[36;1m", "aifo-coder: fork sessions under"),
                                paint(use_color, "\x1b[34;1m", &header_path)
                            );
                            for (sid, panes, _created_at, age_days, base_label, stale) in rows {
                                let base_col = paint(use_color, "\x1b[34;1m", &base_label);
                                if stale {
                                    let stale_col = paint(use_color, "\x1b[33m", "(stale)");
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
                json_escape(&repo_root.display().to_string()),
                sid, panes, created_at, age_days, json_escape(base_label), if *stale { "true" } else { "false" }
            ));
        }
        out.push(']');
        println!("{}", out);
    } else {
        let use_color = color_enabled_stdout();
        let header_path = format!("{}/.aifo-coder/forks", repo_root.display());
        println!(
            "{} {}",
            paint(use_color, "\x1b[36;1m", "aifo-coder: fork sessions under"),
            paint(use_color, "\x1b[34;1m", &header_path)
        );
        for (sid, panes, _created_at, age_days, base_label, stale) in rows {
            let base_col = paint(use_color, "\x1b[34;1m", &base_label);
            if stale {
                let stale_col = paint(use_color, "\x1b[33m", "(stale)");
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

/// Clean fork sessions and panes with safety protections.
/// Returns exit code (0 on success; 1 on refusal or error).
pub fn fork_clean(repo_root: &Path, opts: &ForkCleanOpts) -> std::io::Result<i32> {
    let base = repo_root.join(".aifo-coder").join("forks");
    if !base.exists() {
        eprintln!(
            "aifo-coder: no fork sessions directory at {}",
            base.display()
        );
        return Ok(0);
    }
    let targets: Vec<PathBuf> = if let Some(ref sid) = opts.session {
        let p = base.join(sid);
        if p.exists() {
            vec![p]
        } else {
            Vec::new()
        }
    } else if let Some(days) = opts.older_than_days {
        let now = fork_impl_scan::secs_since_epoch(SystemTime::now());
        fork_impl_scan::session_dirs(&base)
            .into_iter()
            .filter(|sd| {
                let meta = read_file_to_string(&sd.join(".meta.json"));
                let created_at = meta
                    .as_deref()
                    .and_then(|s| crate::fork_meta::extract_value_u64(s, "created_at"))
                    .or_else(|| {
                        fs::metadata(sd)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(fork_impl_scan::secs_since_epoch)
                    })
                    .unwrap_or(0);
                if created_at == 0 {
                    return false;
                }
                let age = (now.saturating_sub(created_at) / 86400) as u64;
                age >= days
            })
            .collect()
    } else if opts.all {
        fork_impl_scan::session_dirs(&base)
    } else {
        eprintln!(
            "aifo-coder: please specify one of --session <sid>, --older-than <days>, or --all."
        );
        return Ok(1);
    };

    if targets.is_empty() {
        eprintln!("aifo-coder: no matching fork sessions to clean.");
        return Ok(0);
    }

    struct PaneStatus {
        dir: PathBuf,
        clean: bool,
        reasons: Vec<String>,
    }

    let mut plan: Vec<(PathBuf, Vec<PaneStatus>)> = Vec::new();
    for sd in &targets {
        let meta = read_file_to_string(&sd.join(".meta.json"));
        let base_commit = meta
            .as_deref()
            .and_then(|s| crate::fork_meta::extract_value_string(s, "base_commit_sha"));
        let mut panes_status = Vec::new();
        for p in fork_impl_scan::pane_dirs_for_session(sd) {
            let pc = fork_impl_panecheck::pane_check(&p, base_commit.as_deref());
            panes_status.push(PaneStatus {
                dir: p,
                clean: pc.clean,
                reasons: pc.reasons,
            });
        }
        plan.push((sd.clone(), panes_status));
    }

    // If JSON + dry-run requested, print plan and exit before confirmation/execution
    if opts.json && opts.dry_run {
        let mut out = String::from("{\"plan\":true,\"sessions\":[");
        for (idx, (sd, panes)) in plan.iter().enumerate() {
            let sid = sd
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("(unknown)");
            let total = panes.len();
            let clean_count = panes.iter().filter(|ps| ps.clean).count();
            let protected = total.saturating_sub(clean_count);
            // Determine deletion scope per session
            let will_delete_session = if opts.force {
                true
            } else if opts.keep_dirty {
                clean_count == total
            } else {
                true
            };
            if idx > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"sid\":{},\"panes_total\":{},\"panes_clean\":{},\"panes_protected\":{},\"will_delete_session\":{}}}",
                json_escape(sid),
                total,
                clean_count,
                protected,
                if will_delete_session { "true" } else { "false" }
            ));
        }
        out.push_str("]}");
        println!("{}", out);
        return Ok(0);
    }

    // Default protection: if any protected pane and neither --force nor --keep-dirty, refuse
    if !opts.force && !opts.keep_dirty {
        let mut protected = 0usize;
        for (_sd, panes) in &plan {
            for ps in panes {
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
            for (sd, panes) in &plan {
                let sid = sd
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("(unknown)");
                for ps in panes {
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
            return Ok(1);
        }
    }

    // Interactive confirmation before deletion (safety prompt)
    if !opts.dry_run && !opts.yes && !opts.json {
        if !atty::is(atty::Stream::Stdin) {
            eprintln!("aifo-coder: refusing to delete without confirmation on non-interactive stdin. Re-run with --yes or --dry-run.");
            return Ok(1);
        }
        let mut del_sessions = 0usize;
        let mut del_panes = 0usize;
        if opts.force {
            del_sessions = plan.len();
            for (_sd, panes) in &plan {
                del_panes += panes.len();
            }
        } else if opts.keep_dirty {
            for (_sd, panes) in &plan {
                let clean_count = panes.iter().filter(|ps| ps.clean).count();
                del_panes += clean_count;
                let remaining = panes.len().saturating_sub(clean_count);
                if remaining == 0 {
                    del_sessions += 1;
                }
            }
        } else {
            del_sessions = plan.len();
            for (_sd, panes) in &plan {
                del_panes += panes.len();
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
                return Ok(1);
            }
        }
    }

    // Execute deletions (or print in dry-run)
    let mut deleted_sessions_count: usize = 0;
    let mut deleted_panes_count: usize = 0;

    for (sd, panes) in &plan {
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
                // count all panes removed and the session
                deleted_panes_count += panes.len();
                deleted_sessions_count += 1;
                // Stop toolchain sidecars and remove session network (best-effort)
                toolchain_cleanup_session(&sid, false);
                let _ = fs::remove_dir_all(sd);
                // Success message
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
                    // Stop toolchain sidecars and remove session network (best-effort)
                    toolchain_cleanup_session(&sid, false);
                    let _ = fs::remove_dir_all(sd);
                    // Success message
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
                    // Collect current branches for remaining panes
                    let mut branches: Vec<String> = Vec::new();
                    for p in &remaining {
                        if let Ok(out) = Command::new("git")
                            .arg("-C")
                            .arg(p)
                            .arg("rev-parse")
                            .arg("--abbrev-ref")
                            .arg("HEAD")
                            .stdout(Stdio::piped())
                            .stderr(Stdio::null())
                            .output()
                        {
                            if out.status.success() {
                                let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                if !b.is_empty() {
                                    branches.push(b);
                                }
                            }
                        }
                    }

                    // Enrich metadata with prior fields and use valid JSON escaping
                    let prev = read_file_to_string(&sd.join(".meta.json"));
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
                    // Kept session summary
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
                // Stop toolchain sidecars and remove session network (best-effort)
                toolchain_cleanup_session(&sid, false);
                let _ = fs::remove_dir_all(sd);
                // Success message
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

    if !opts.yes && !opts.dry_run {
        // nothing interactive implemented; --yes is accepted to match CLI but not required
    }

    if opts.json && !opts.dry_run {
        println!(
            "{{\"executed\":true,\"deleted_sessions\":{},\"deleted_panes\":{}}}",
            deleted_sessions_count, deleted_panes_count
        );
    }

    Ok(0)
}

/// Print a notice about stale fork sessions for the current repository (quiet; best-effort).
pub fn fork_print_stale_notice() {
    // Delegate to private helper module (no behavior change).
    fork_impl_notice::fork_print_stale_notice_impl();
}

/// Auto-clean clean fork sessions older than the stale threshold when AIFO_CODER_FORK_AUTOCLEAN=1 is set.
/// - Only removes sessions where all panes are clean (no dirty, ahead, or base-unknown panes).
/// - Threshold in days is taken from AIFO_CODER_FORK_STALE_DAYS (default 30).
/// - Prints a concise summary of deletions and survivors.
pub fn fork_autoclean_if_enabled() {
    // Delegate to private helper module (no behavior change).
    fork_impl_notice::fork_autoclean_if_enabled_impl();
}

fn collect_pane_branches(panes: &[(PathBuf, String)]) -> std::io::Result<Vec<(PathBuf, String)>> {
    let mut pane_branches: Vec<(PathBuf, String)> = Vec::new();
    for (pdir, branch_hint) in panes {
        let actual_branch = Command::new("git")
            .arg("-C")
            .arg(pdir)
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .filter(|s| !s.is_empty() && s.as_str() != "HEAD")
            .unwrap_or_else(|| branch_hint.clone());
        if actual_branch.is_empty() || actual_branch == "HEAD" {
            continue;
        }
        pane_branches.push((pdir.clone(), actual_branch));
    }
    if pane_branches.is_empty() {
        return Err(std::io::Error::other(
            "no pane branches to process (empty pane set or detached HEAD)",
        ));
    }
    Ok(pane_branches)
}

fn preflight_clean_working_tree(repo_root: &Path) -> std::io::Result<()> {
    let dirty = match Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-uall")
        .output()
    {
        Ok(o) => {
            if !o.status.success() {
                true
            } else {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines().any(|line| {
                    let path = if line.len() > 3 { &line[3..] } else { "" };
                    let ignore = path == ".aifo-coder"
                        || path.starts_with(".aifo-coder/")
                        || path.starts_with(".aifo-coder\\");
                    !ignore && !path.is_empty()
                })
            }
        }
        Err(_) => true,
    };
    if dirty {
        return Err(std::io::Error::other(
            "octopus merge requires a clean working tree in the original repository",
        ));
    }
    Ok(())
}

fn compose_merge_message(
    repo_root: &Path,
    pane_branches: &[(PathBuf, String)],
    base_ref_or_sha: &str,
) -> String {
    let mut merge_message = String::new();

    let mut summary_parts: Vec<String> = Vec::new();
    for (_p, br) in pane_branches {
        let subj_out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("log")
            .arg("--no-merges")
            .arg("--pretty=format:%s")
            .arg(format!("{}..{}", base_ref_or_sha, br))
            .output()
            .ok();
        if let Some(o) = subj_out {
            if o.status.success() {
                let body = String::from_utf8_lossy(&o.stdout);
                for s in body.lines() {
                    let mut t = s.trim().to_string();
                    if t.is_empty() {
                        continue;
                    }
                    if let Some(pos) = t.find(':') {
                        let (prefix, rest) = t.split_at(pos);
                        let pref = prefix.to_ascii_lowercase();
                        if [
                            "feat", "fix", "docs", "style", "refactor", "perf", "test", "chore",
                            "build", "ci", "revert",
                        ]
                        .contains(&pref.as_str())
                        {
                            t = rest.trim_start_matches(':').trim().to_string();
                        }
                    }
                    t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if t.is_empty() {
                        continue;
                    }
                    if !summary_parts.iter().any(|e| e.eq_ignore_ascii_case(&t)) {
                        summary_parts.push(t);
                    }
                }
            }
        }
    }
    let mut summary_line = if summary_parts.is_empty() {
        format!("Octopus merge of {} branch(es)", pane_branches.len())
    } else {
        let joined = summary_parts.join(" / ");
        if joined.len() > 160 {
            format!("{} …", &joined[..160].trim_end())
        } else {
            joined
        }
    };
    if !summary_line
        .to_ascii_lowercase()
        .starts_with("octopus merge")
    {
        summary_line = format!("Octopus merge: {}", summary_line);
    }
    merge_message.push_str(&format!(
        "{}\n\nBranch summaries relative to {}:\n",
        summary_line, base_ref_or_sha
    ));

    for (_p, br) in pane_branches {
        let log_out = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("log")
            .arg("--no-merges")
            .arg("--pretty=format:%h %s")
            .arg(format!("{}..{}", base_ref_or_sha, br))
            .output()
            .ok();
        merge_message.push_str(&format!("- branch '{}':\n", br));
        if let Some(o) = log_out {
            if o.status.success() {
                let body = String::from_utf8_lossy(&o.stdout);
                let mut has_any = false;
                for line in body.lines() {
                    let t = line.trim();
                    if !t.is_empty() {
                        has_any = true;
                        merge_message.push_str("  * ");
                        merge_message.push_str(t);
                        merge_message.push('\n');
                    }
                }
                if !has_any {
                    merge_message.push_str("  (no changes)\n");
                }
            } else {
                merge_message.push_str("  (unable to summarize changes)\n");
            }
        } else {
            merge_message.push_str("  (unable to summarize changes)\n");
        }
        merge_message.push('\n');
    }

    merge_message
}

/// Merge helper: fetch pane branches and optionally octopus-merge them.
pub fn fork_merge_branches(
    repo_root: &Path,
    sid: &str,
    panes: &[(PathBuf, String)],
    base_ref_or_sha: &str,
    strategy: crate::MergingStrategy,
    verbose: bool,
    dry_run: bool,
) -> std::io::Result<()> {
    if matches!(strategy, crate::MergingStrategy::None) {
        return Ok(());
    }

    let pane_branches = collect_pane_branches(panes)?;

    // 1) Fetch each pane branch back into the original repo as a local branch with the same name
    for (pdir, br) in &pane_branches {
        let pdir_str = pdir.display().to_string();
        let refspec = format!("{b}:refs/heads/{b}", b = br);
        let args = vec![
            "git".to_string(),
            "-C".to_string(),
            repo_root.display().to_string(),
            "-c".to_string(),
            "protocol.file.allow=always".to_string(),
            "fetch".to_string(),
            "--no-tags".to_string(),
            pdir_str.clone(),
            refspec.clone(),
        ];
        if verbose || dry_run {
            eprintln!("aifo-coder: git: {}", shell_join(&args));
        }
        if !dry_run {
            let mut cmd = Command::new("git");
            cmd.arg("-C")
                .arg(repo_root)
                .arg("-c")
                .arg("protocol.file.allow=always")
                .arg("fetch")
                .arg("--no-tags")
                .arg(&pdir_str)
                .arg(&refspec);
            if !verbose {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
            let st = cmd.status()?;
            if !st.success() {
                return Err(std::io::Error::other(format!(
                    "git fetch failed for pane {} (branch {})",
                    pdir.display(),
                    br
                )));
            }
        }
    }

    if matches!(strategy, crate::MergingStrategy::Fetch) {
        // Update session metadata with fetched branches
        let fetched_names: Vec<String> = pane_branches.iter().map(|(_p, b)| b.clone()).collect();
        let _ = crate::fork_meta::append_fields_compact(
            repo_root,
            sid,
            &format!(
                "\"merge_strategy\":{},\"fetched\":[{}]",
                json_escape("fetch"),
                fetched_names
                    .iter()
                    .map(|b| json_escape(b))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        return Ok(());
    }

    preflight_clean_working_tree(repo_root)?;

    // Checkout or create merge/<sid> at base_ref_or_sha
    let target = format!("merge/{}", sid);
    let checkout_args = vec![
        "git".to_string(),
        "-C".to_string(),
        repo_root.display().to_string(),
        "checkout".to_string(),
        "-B".to_string(),
        target.clone(),
        base_ref_or_sha.to_string(),
    ];
    if verbose || dry_run {
        eprintln!("aifo-coder: git: {}", shell_join(&checkout_args));
    }
    if !dry_run {
        let st = Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("checkout")
            .arg("-B")
            .arg(&target)
            .arg(base_ref_or_sha)
            .status()?;
        if !st.success() {
            return Err(std::io::Error::other(
                "failed to checkout merge target branch",
            ));
        }
    }

    let merge_message = compose_merge_message(repo_root, &pane_branches, base_ref_or_sha);

    // Prepare merge message file path; write it unless dry_run
    let mut merge_msg_path: Option<std::path::PathBuf> = None;
    let msg_path =
        std::env::temp_dir().join(format!("aifo-merge-{}-{}.txt", sid, std::process::id()));
    if verbose {
        eprintln!(
            "aifo-coder: preparing octopus merge message at {}",
            msg_path.display()
        );
    }
    if !dry_run {
        if fs::write(&msg_path, &merge_message).is_ok() {
            merge_msg_path = Some(msg_path.clone());
        } else if verbose {
            eprintln!("aifo-coder: warning: failed to write merge message file; falling back to default message");
        }
    }

    // Perform octopus merge (use -F <file> when available)
    let mut merge_args = vec![
        "git".to_string(),
        "-C".to_string(),
        repo_root.display().to_string(),
        "merge".to_string(),
        "--no-ff".to_string(),
        "--no-edit".to_string(),
    ];
    if merge_msg_path.is_some() || dry_run {
        merge_args.push("-F".to_string());
        merge_args.push(msg_path.display().to_string());
    }
    for (_p, br) in &pane_branches {
        merge_args.push(br.clone());
    }
    if verbose || dry_run {
        eprintln!("aifo-coder: git: {}", shell_join(&merge_args));
    }
    if !dry_run {
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(repo_root)
            .arg("merge")
            .arg("--no-ff")
            .arg("--no-edit");
        if let Some(ref p) = merge_msg_path {
            cmd.arg("-F").arg(p);
        }
        for (_p, br) in &pane_branches {
            cmd.arg(br);
        }
        let st = cmd.status()?;
        // cleanup temp file best-effort
        if let Some(p) = merge_msg_path {
            let _ = fs::remove_file(p);
        }
        if !st.success() {
            // Record failed octopus merge in metadata
            let fetched_names: Vec<String> =
                pane_branches.iter().map(|(_p, b)| b.clone()).collect();
            let _ = crate::fork_meta::append_fields_compact(
                repo_root,
                sid,
                &format!(
                    "\"merge_strategy\":{},\"fetched\":[{}],\"merge_failed\":true",
                    json_escape("octopus"),
                    fetched_names
                        .iter()
                        .map(|b| json_escape(b))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            return Err(std::io::Error::other(
                "octopus merge failed (conflicts likely). Resolve manually and retry.",
            ));
        }
    }

    // Update session metadata with octopus merge result
    {
        let fetched_names: Vec<String> = pane_branches.iter().map(|(_p, b)| b.clone()).collect();
        let merge_commit_sha = if !dry_run {
            Command::new("git")
                .arg("-C")
                .arg(repo_root)
                .arg("rev-parse")
                .arg("--verify")
                .arg("HEAD")
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        let merged_at = fork_impl_scan::secs_since_epoch(SystemTime::now());
        let _ = crate::fork_meta::append_fields_compact(
            repo_root,
            sid,
            &format!(
                "\"merge_strategy\":{},\"fetched\":[{}],\"merge_target\":{},\"merge_commit_sha\":{},\"merged_at\":{}",
                json_escape("octopus"),
                fetched_names
                    .iter()
                    .map(|b| json_escape(b))
                    .collect::<Vec<_>>()
                    .join(", "),
                json_escape(&format!("merge/{}", sid)),
                json_escape(&merge_commit_sha),
                merged_at
            ),
        );
    }

    // Delete merged pane branches locally (best-effort)
    if !dry_run {
        let mut del = Command::new("git");
        del.arg("-C").arg(repo_root).arg("branch").arg("-D");
        for (_p, br) in &pane_branches {
            del.arg(br);
        }
        if !verbose {
            del.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = del.status();
    } else if verbose {
        let mut preview = vec![
            "git".to_string(),
            "-C".to_string(),
            repo_root.display().to_string(),
            "branch".to_string(),
            "-D".to_string(),
        ];
        for (_p, br) in &pane_branches {
            preview.push(br.clone());
        }
        eprintln!("aifo-coder: git: {}", shell_join(&preview));
    }

    Ok(())
}

/// Convenience wrapper: read pane directories and base from session metadata, then merge.
pub fn fork_merge_branches_by_session(
    repo_root: &Path,
    sid: &str,
    strategy: crate::MergingStrategy,
    verbose: bool,
    dry_run: bool,
) -> std::io::Result<()> {
    if matches!(strategy, crate::MergingStrategy::None) {
        return Ok(());
    }
    let session_dir = fork_session_dir(repo_root, sid);
    if !session_dir.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "fork session directory not found: {}",
                session_dir.display()
            ),
        ));
    }

    // Gather pane dirs
    let panes_dirs = fork_impl_scan::pane_dirs_for_session(&session_dir);
    if panes_dirs.is_empty() {
        return Err(std::io::Error::other(
            "no pane directories found under session",
        ));
    }

    // Determine base_ref_or_sha from .meta.json if present; otherwise fallback to HEAD
    let meta_path = session_dir.join(".meta.json");
    let meta = read_file_to_string(&meta_path);
    let base_ref_or_sha = meta
        .as_deref()
        .and_then(|s| crate::fork_meta::extract_value_string(s, "base_ref_or_sha"))
        .unwrap_or_else(|| "HEAD".to_string());

    // Determine each pane's current branch
    let mut panes: Vec<(PathBuf, String)> = Vec::new();
    for p in panes_dirs {
        let branch = Command::new("git")
            .arg("-C")
            .arg(&p)
            .arg("rev-parse")
            .arg("--abbrev-ref")
            .arg("HEAD")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        if !branch.is_empty() && branch != "HEAD" {
            panes.push((p, branch));
        }
    }

    if panes.is_empty() {
        return Err(std::io::Error::other(
            "no pane branches found (detached HEAD?)",
        ));
    }

    fork_merge_branches(
        repo_root,
        sid,
        &panes,
        &base_ref_or_sha,
        strategy,
        verbose,
        dry_run,
    )
}
