use std::fs;
use std::path::PathBuf;

use crate::fork_meta;
use crate::json_escape;

use crate::fork::fork_impl_panecheck;
use crate::fork::fork_impl_scan;

#[derive(Clone)]
pub struct PaneStatus {
    pub dir: PathBuf,
    pub clean: bool,
    pub reasons: Vec<String>,
}

#[derive(Clone)]
pub struct SessionPlan {
    pub dir: PathBuf,
    pub panes: Vec<PaneStatus>,
}

/// Build a plan for the given target session directories.
pub fn build_plan_for_targets(targets: &[PathBuf]) -> Vec<SessionPlan> {
    let mut plan: Vec<SessionPlan> = Vec::new();
    for sd in targets {
        let meta = fs::read_to_string(sd.join(".meta.json")).ok();
        let base_commit = meta
            .as_deref()
            .and_then(|s| fork_meta::extract_value_string(s, "base_commit_sha"));
        let mut panes_status = Vec::new();
        for p in fork_impl_scan::pane_dirs_for_session(sd) {
            let pc = fork_impl_panecheck::pane_check(&p, base_commit.as_deref());
            panes_status.push(PaneStatus {
                dir: p,
                clean: pc.clean,
                reasons: pc.reasons,
            });
        }
        plan.push(SessionPlan {
            dir: sd.clone(),
            panes: panes_status,
        });
    }
    plan
}

/// Print dry-run plan as JSON (identical to previous implementation).
pub fn print_dry_run_json(plan: &[SessionPlan], opts: &crate::ForkCleanOpts) {
    let mut out = String::from("{\"plan\":true,\"sessions\":[");
    for (idx, sp) in plan.iter().enumerate() {
        let sid = sp
            .dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unknown)");
        let total = sp.panes.len();
        let clean_count = sp.panes.iter().filter(|ps| ps.clean).count();
        let protected = total.saturating_sub(clean_count);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn have_git() -> bool {
        Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn test_plan_marks_dirty_and_base_unknown() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempdir().expect("tmpdir");
        let repo = td.path();

        // Layout: forks/sid/pane-1 (git repo)
        let session = repo.join("sid");
        let pane = session.join("pane-1");
        fs::create_dir_all(&pane).unwrap();

        // Initialize git repo
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        let _ = Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&pane)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&pane)
            .status();
        fs::write(pane.join("a.txt"), "a\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "c1"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        // dirty change (uncommitted)
        fs::write(pane.join("b.txt"), "b\n").unwrap();

        // No base_commit_sha in meta -> base_unknown
        fs::write(session.join(".meta.json"), "{ \"created_at\": 0 }").unwrap();

        let plan = build_plan_for_targets(&[session.clone()]);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].panes.len(), 1);
        let reasons = &plan[0].panes[0].reasons;
        assert!(
            reasons.iter().any(|r| r == "dirty"),
            "expected 'dirty' in reasons: {:?}",
            reasons
        );
        assert!(
            reasons.iter().any(|r| r == "base-unknown"),
            "expected 'base-unknown' in reasons: {:?}",
            reasons
        );
    }

    #[test]
    fn test_plan_marks_ahead_when_base_known() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempdir().expect("tmpdir");
        let repo = td.path();

        // Layout: forks/sid/pane-1 (git repo)
        let session = repo.join("sid2");
        let pane = session.join("pane-1");
        fs::create_dir_all(&pane).unwrap();

        // Initialize git repo
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        let _ = Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&pane)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&pane)
            .status();
        fs::write(pane.join("a.txt"), "a\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "c1"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        // Capture initial commit SHA for base
        let head1 = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap();
        let base_sha = String::from_utf8_lossy(&head1.stdout).trim().to_string();

        // New commit to make HEAD ahead of base
        fs::write(pane.join("b.txt"), "b\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "c2"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        let meta = format!(
            "{{ \"created_at\": 0, \"base_commit_sha\": \"{}\" }}",
            base_sha
        );
        fs::write(session.join(".meta.json"), meta).unwrap();

        let plan = build_plan_for_targets(&[session.clone()]);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].panes.len(), 1);
        let reasons = &plan[0].panes[0].reasons;
        assert!(
            reasons.iter().any(|r| r == "ahead"),
            "expected 'ahead' in reasons: {:?}",
            reasons
        );
    }
}
