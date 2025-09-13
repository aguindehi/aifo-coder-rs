use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Session metadata in the on-disk JSON file (order preserved by manual writer).
pub struct SessionMeta<'a> {
    pub created_at: u64,
    pub base_label: &'a str,
    pub base_ref_or_sha: &'a str,
    pub base_commit_sha: String, // computed per rules
    pub panes: usize,
    pub pane_dirs: Vec<PathBuf>,
    pub branches: Vec<String>,
    pub layout: &'a str,
    pub snapshot_sha: Option<&'a str>,
}

/// Compute the base_commit_sha per current main.rs rules and write .meta.json (single line).
pub fn write_initial_meta(repo_root: &Path, sid: &str, m: &SessionMeta<'_>) -> io::Result<()> {
    let session_dir = aifo_coder::fork_session_dir(repo_root, sid);
    let _ = fs::create_dir_all(&session_dir);

    // Manual JSON to preserve key order and minimize diffs.
    let pane_dirs_vec: Vec<String> = m
        .pane_dirs
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    let branches_vec: Vec<String> = m.branches.clone();

    // Compute base_commit_sha per current main.rs rules when not provided:
    // - Use snapshot sha when present
    // - Else: rev-parse --verify base_ref_or_sha
    // - Else: fallback to HEAD sha from fork_base_info()
    let base_commit_sha = if !m.base_commit_sha.is_empty() {
        m.base_commit_sha.clone()
    } else if let Some(snap) = m.snapshot_sha {
        snap.to_string()
    } else {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .arg("rev-parse")
            .arg("--verify")
            .arg(m.base_ref_or_sha)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok();
        if let Some(o) = out {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout).trim().to_string()
            } else {
                aifo_coder::fork_base_info(repo_root)
                    .map(|(_, _, head)| head)
                    .unwrap_or_default()
            }
        } else {
            aifo_coder::fork_base_info(repo_root)
                .map(|(_, _, head)| head)
                .unwrap_or_default()
        }
    };

    let mut s = format!(
        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
        m.created_at,
        aifo_coder::json_escape(m.base_label),
        aifo_coder::json_escape(m.base_ref_or_sha),
        aifo_coder::json_escape(&base_commit_sha),
        m.panes,
        pane_dirs_vec.iter().map(|x| aifo_coder::json_escape(x).to_string()).collect::<Vec<_>>().join(", "),
        branches_vec.iter().map(|x| aifo_coder::json_escape(x).to_string()).collect::<Vec<_>>().join(", "),
        aifo_coder::json_escape(m.layout)
    );
    if let Some(snap) = m.snapshot_sha {
        s.push_str(&format!(
            ", \"snapshot_sha\": {}",
            aifo_coder::json_escape(snap)
        ));
    }
    s.push_str(" }");
    fs::write(session_dir.join(".meta.json"), s)
}

/// Read, minimally update panes_created, pane_dirs, branches, and preserve other fields.
pub fn update_panes_created(
    repo_root: &Path,
    sid: &str,
    created_count: usize,
    existing: &[(PathBuf, String)],
    snapshot_sha: Option<&str>,
    layout: &str,
) -> io::Result<()> {
    let session_dir = aifo_coder::fork_session_dir(repo_root, sid);
    let meta_path = session_dir.join(".meta.json");

    // Build new arrays from on-disk existing panes
    let pane_dirs_vec: Vec<String> = existing
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect();
    let branches_vec: Vec<String> = existing.iter().map(|(_, b)| b.clone()).collect();

    // Parse a few existing fields (best-effort) to preserve them; fall back to empty/defaults.
    let text = fs::read_to_string(&meta_path).unwrap_or_default();
    let created_at = extract_value_u64(&text, "created_at").unwrap_or(0);
    let base_label = extract_value_string(&text, "base_label").unwrap_or_else(|| "".to_string());
    let base_ref_or_sha =
        extract_value_string(&text, "base_ref_or_sha").unwrap_or_else(|| "".to_string());
    let base_commit_sha =
        extract_value_string(&text, "base_commit_sha").unwrap_or_else(|| "".to_string());
    let panes = extract_value_u64(&text, "panes").unwrap_or(existing.len() as u64) as usize;
    let snapshot_old = extract_value_string(&text, "snapshot_sha");

    // Recompose JSON with fixed key order and updated arrays.
    let mut s = format!(
        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
        created_at,
        aifo_coder::json_escape(&base_label),
        aifo_coder::json_escape(&base_ref_or_sha),
        aifo_coder::json_escape(&base_commit_sha),
        panes,
        created_count,
        pane_dirs_vec.iter().map(|x| aifo_coder::json_escape(x).to_string()).collect::<Vec<_>>().join(", "),
        branches_vec.iter().map(|x| aifo_coder::json_escape(x).to_string()).collect::<Vec<_>>().join(", "),
        aifo_coder::json_escape(layout)
    );
    if let Some(snap) = snapshot_old.as_deref().or(snapshot_sha) {
        s.push_str(&format!(
            ", \"snapshot_sha\": {}",
            aifo_coder::json_escape(snap)
        ));
    }
    s.push_str(" }");

    fs::write(meta_path, s)
}

// Minimal JSON string parsers for specific fields
fn extract_value_string(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\":", key);
    let pos = text.find(&needle)?;
    let rest = &text[pos + needle.len()..];
    let rest = rest.trim_start();
    if rest.starts_with('"') {
        let mut out = String::new();
        let mut chars = rest[1..].chars();
        while let Some(ch) = chars.next() {
            if ch == '"' {
                break;
            }
            out.push(ch);
        }
        Some(out)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as aifo_coder;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs()
    }

    #[test]
    fn test_write_initial_meta_uses_snapshot_and_preserves_key_order() {
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        let sid = "sid-meta";
        let created_at = now_secs();

        let m = SessionMeta {
            created_at,
            base_label: "main",
            base_ref_or_sha: "main",
            base_commit_sha: String::new(),
            panes: 2,
            pane_dirs: vec![root.join("p1"), root.join("p2")],
            branches: vec!["b1".to_string(), "b2".to_string()],
            layout: "tiled",
            snapshot_sha: Some("abc123"),
        };
        write_initial_meta(&root, sid, &m).expect("write meta");

        let meta_path = aifo_coder::fork_session_dir(&root, sid).join(".meta.json");
        let txt = std::fs::read_to_string(&meta_path).expect("read meta");
        // Keys must appear in this order
        let keys = [
            "\"created_at\":",
            "\"base_label\":",
            "\"base_ref_or_sha\":",
            "\"base_commit_sha\":",
            "\"panes\":",
            "\"pane_dirs\":",
            "\"branches\":",
            "\"layout\":",
            "\"snapshot_sha\":",
        ];
        let mut pos = 0usize;
        for k in keys {
            if let Some(p) = txt[pos..].find(k) {
                pos += p + k.len();
            } else {
                panic!("missing key or wrong order: {} in {}", k, txt);
            }
        }
        // base_commit_sha should be the snapshot sha since provided
        assert!(
            txt.contains("\"base_commit_sha\": \"abc123\""),
            "expected base_commit_sha to use snapshot sha, got: {}",
            txt
        );
    }

    #[test]
    fn test_update_panes_created_updates_counts_and_preserves_fields() {
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        let sid = "sid-update";
        let created_at = now_secs();

        // initial meta with snapshot to avoid git calls
        let m = SessionMeta {
            created_at,
            base_label: "main",
            base_ref_or_sha: "main",
            base_commit_sha: String::new(),
            panes: 3,
            pane_dirs: vec![root.join("x1"), root.join("x2"), root.join("x3")],
            branches: vec!["b1".to_string(), "b2".to_string(), "b3".to_string()],
            layout: "tiled",
            snapshot_sha: Some("cafebabe"),
        };
        write_initial_meta(&root, sid, &m).expect("write meta");

        // Only one pane exists on disk
        let p1 = root.join("x1");
        std::fs::create_dir_all(&p1).unwrap();
        let existing = vec![(p1.clone(), "b1".to_string())];

        update_panes_created(
            &root,
            sid,
            existing.len(),
            &existing,
            Some("cafebabe"),
            "tiled",
        )
        .expect("update");

        let meta_path = aifo_coder::fork_session_dir(&root, sid).join(".meta.json");
        let txt = std::fs::read_to_string(&meta_path).expect("read updated meta");

        assert!(
            txt.contains("\"panes_created\": 1"),
            "expected panes_created=1, got: {}",
            txt
        );
        assert!(
            txt.contains("\"snapshot_sha\": \"cafebabe\""),
            "expected snapshot_sha preserved, got: {}",
            txt
        );
        // base fields must remain present
        for k in [
            "\"created_at\":",
            "\"base_label\":",
            "\"base_ref_or_sha\":",
            "\"base_commit_sha\":",
            "\"panes\":",
            "\"pane_dirs\":",
            "\"branches\":",
            "\"layout\":",
        ] {
            assert!(
                txt.contains(k),
                "missing key after update: {} in {}",
                k,
                txt
            );
        }
    }

    #[test]
    fn test_write_initial_meta_uses_rev_parse_when_no_snapshot() {
        // Skip if git is not available on this host
        let git_ok = std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !git_ok {
            eprintln!("skipping: git not found in PATH");
            return;
        }

        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        // Initialize a git repo with a single commit
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&root)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&root)
            .status();
        std::fs::write(root.join("file.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());

        // Resolve HEAD sha for comparison
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&root)
            .output()
            .unwrap();
        let head_sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert!(!head_sha.is_empty(), "HEAD sha must be non-empty");

        // Write initial meta without snapshot; base_ref_or_sha points to HEAD
        let sid = "sid-rev-parse";
        let m = SessionMeta {
            created_at: 0,
            base_label: "main",
            base_ref_or_sha: "HEAD",
            base_commit_sha: String::new(),
            panes: 1,
            pane_dirs: vec![root.join("p1")],
            branches: vec!["b1".to_string()],
            layout: "tiled",
            snapshot_sha: None,
        };
        write_initial_meta(&root, sid, &m).expect("write meta");

        // Verify base_commit_sha equals HEAD sha
        let meta_path = aifo_coder::fork_session_dir(&root, sid).join(".meta.json");
        let txt = std::fs::read_to_string(&meta_path).expect("read meta");
        assert!(
            txt.contains(&format!("\"base_commit_sha\": \"{}\"", head_sha)),
            "expected base_commit_sha to match HEAD sha, got: {}",
            txt
        );
    }

    #[test]
    fn test_write_initial_meta_falls_back_to_head_when_rev_parse_fails() {
        // Skip if git is not available on this host
        let git_ok = std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !git_ok {
            eprintln!("skipping: git not found in PATH");
            return;
        }

        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        // Initialize a git repo with a single commit
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&root)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&root)
            .status();
        std::fs::write(root.join("file.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());

        // Obtain HEAD via fork_base_info (the fallback)
        let (_label, _base, head_sha) =
            aifo_coder::fork_base_info(&root).expect("fork_base_info head");

        // Write initial meta with a non-existent ref to trigger fallback
        let sid = "sid-fallback-head";
        let m = SessionMeta {
            created_at: 0,
            base_label: "main",
            base_ref_or_sha: "does-not-exist",
            base_commit_sha: String::new(),
            panes: 1,
            pane_dirs: vec![root.join("p1")],
            branches: vec!["b1".to_string()],
            layout: "tiled",
            snapshot_sha: None,
        };
        write_initial_meta(&root, sid, &m).expect("write meta");

        // Verify base_commit_sha equals fallback HEAD sha
        let meta_path = aifo_coder::fork_session_dir(&root, sid).join(".meta.json");
        let txt = std::fs::read_to_string(&meta_path).expect("read meta");
        assert!(
            txt.contains(&format!("\"base_commit_sha\": \"{}\"", head_sha)),
            "expected base_commit_sha to fall back to HEAD sha, got: {}",
            txt
        );
    }
}

fn extract_value_u64(text: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{}\":", key);
    let pos = text.find(&needle)?;
    let rest = &text[pos + needle.len()..];
    let rest = rest.trim_start();
    let mut num = String::new();
    for ch in rest.chars() {
        if ch.is_ascii_digit() {
            num.push(ch);
        } else {
            break;
        }
    }
    num.parse::<u64>().ok()
}
