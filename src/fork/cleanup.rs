use std::path::{Path, PathBuf};

/// Remove or keep created pane directories and update .meta.json with panes_created.
/// When print_recovery_example is true, also print the fixed "Example recovery:" guidance block.
pub fn cleanup_and_update_meta(
    repo_root: &Path,
    sid: &str,
    clones: &[(PathBuf, String)],
    keep_on_failure: bool,
    session_dir: &Path,
    snapshot_sha: Option<&str>,
    layout: &str,
    print_recovery_example: bool,
) {
    if !keep_on_failure {
        for (dir, _) in clones {
            let _ = std::fs::remove_dir_all(dir);
        }
        println!(
            "Removed all created pane directories under {}.",
            session_dir.display()
        );
    } else {
        println!(
            "Clones remain under {} for recovery.",
            session_dir.display()
        );
        if print_recovery_example {
            if let Some((first_dir, first_branch)) = clones.first() {
                println!("Example recovery:");
                println!("  git -C \"{}\" status", first_dir.display());
                println!(
                    "  git -C \"{}\" log --oneline --decorate -n 20",
                    first_dir.display()
                );
                println!(
                    "  git -C \"{}\" remote add fork-{}-1 \"{}\"",
                    repo_root.display(),
                    sid,
                    first_dir.display()
                );
                println!(
                    "  git -C \"{}\" fetch fork-{}-1 {}",
                    repo_root.display(),
                    sid,
                    first_branch
                );
            }
        }
    }

    // Update metadata with panes_created based on panes that exist on disk
    let existing: Vec<(PathBuf, String)> = clones
        .iter()
        .filter(|(p, _)| p.exists())
        .map(|(p, b)| (p.clone(), b.clone()))
        .collect();
    let _ = crate::fork::meta::update_panes_created(
        repo_root,
        sid,
        existing.len(),
        &existing,
        snapshot_sha,
        layout,
    );
}
