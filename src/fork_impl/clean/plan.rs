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
