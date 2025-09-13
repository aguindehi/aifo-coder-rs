use std::path::{Path, PathBuf};

/// Print the fork session header, created clones line, optional snapshot or warning,
/// optional dissociate note, and a trailing blank line. Text and colorization matches main.rs exactly.
pub fn print_header(
    sid: &str,
    base_label: &str,
    base_ref_or_sha: &str,
    session_dir: &Path,
    panes: usize,
    snapshot_sha: Option<&str>,
    include_dirty_requested: bool,
    print_dissoc_note: bool,
    use_color_out: bool,
) {
    if use_color_out {
        println!(
            "\x1b[36;1maifo-coder:\x1b[0m fork session \x1b[32;1m{}\x1b[0m on base \x1b[34;1m{}\x1b[0m (\x1b[34m{}\x1b[0m)",
            sid, base_label, base_ref_or_sha
        );
    } else {
        println!(
            "aifo-coder: fork session {} on base {} ({})",
            sid, base_label, base_ref_or_sha
        );
    }
    println!();

    if use_color_out {
        println!(
            "created \x1b[36;1m{}\x1b[0m clones under \x1b[34;1m{}\x1b[0m",
            panes,
            session_dir.display()
        );
    } else {
        println!("created {} clones under {}", panes, session_dir.display());
    }

    if let Some(snap) = snapshot_sha {
        if use_color_out {
            println!(
                "\x1b[32mincluded dirty working tree via snapshot {}\x1b[0m",
                snap
            );
        } else {
            println!("included dirty working tree via snapshot {}", snap);
        }
    } else if include_dirty_requested {
        if use_color_out {
            println!("\x1b[33mwarning:\x1b[0m requested --fork-include-dirty, but snapshot failed; dirty changes not included.");
        } else {
            println!("warning: requested --fork-include-dirty, but snapshot failed; dirty changes not included.");
        }
    }

    if print_dissoc_note {
        if use_color_out {
            println!("\x1b[90mnote: clones reference the base repo’s object store; avoid pruning base objects until done.\x1b[0m");
        } else {
            println!("note: clones reference the base repo’s object store; avoid pruning base objects until done.");
        }
    }

    println!();
}

/// Print per-pane information blocks exactly as in main.rs and ensure per-pane state subdirs exist.
pub fn print_per_pane_blocks(
    agent: &str,
    sid: &str,
    state_base: &Path,
    clones: &[(PathBuf, String)],
    use_color_out: bool,
) {
    for (idx, (pane_dir, branch)) in clones.iter().enumerate() {
        let i = idx + 1;
        let cname = crate::fork::env::pane_container_name(agent, sid, i);
        let state_dir = crate::fork::env::pane_state_dir(state_base, sid, i);
        let _ = std::fs::create_dir_all(state_dir.join(".aider"));
        let _ = std::fs::create_dir_all(state_dir.join(".codex"));
        let _ = std::fs::create_dir_all(state_dir.join(".crush"));
        if use_color_out {
            println!(
                "[\x1b[36;1m{}\x1b[0m] folder=\x1b[34m{}\x1b[0m",
                i,
                pane_dir.display()
            );
            println!("    branch=\x1b[32m{}\x1b[0m", branch);
            println!("    state=\x1b[90m{}\x1b[0m", state_dir.display());
            println!("    container=\x1b[35m{}\x1b[0m", cname);
            println!();
        } else {
            println!("[{}] folder={}", i, pane_dir.display());
            println!("    branch={}", branch);
            println!("    state={}", state_dir.display());
            println!("    container={}", cname);
            println!();
        }
    }
}
