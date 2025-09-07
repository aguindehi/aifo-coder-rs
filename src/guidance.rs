pub fn print_inspect_merge_guidance(
    repo_root: &std::path::Path,
    sid: &str,
    base_label: &str,
    base_ref_or_sha: &str,
    clones: &[(std::path::PathBuf, String)],
    use_color_header: bool,
    include_remote_examples: bool,
    extra_spacing_before_wrapper: bool,
) {
    if use_color_header {
        println!("\x1b[1mTo inspect and merge changes, you can run:\x1b[0m");
    } else {
        println!("To inspect and merge changes, you can run:");
    }
    if let Some((first_dir, first_branch)) = clones.first() {
        println!("  git -C \"{}\" status", first_dir.display());
        println!(
            "  git -C \"{}\" log --oneline --decorate --graph -n 20",
            first_dir.display()
        );
        if include_remote_examples {
            println!(
                "  git -C \"{}\" remote add fork-{}-1 \"{}\"  # once",
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
            if base_label != "detached" {
                println!(
                    "  git -C \"{}\" checkout {}",
                    repo_root.display(),
                    base_ref_or_sha
                );
                println!(
                    "  git -C \"{}\" merge --no-ff {}",
                    repo_root.display(),
                    first_branch
                );
            }
        }
    }
    if extra_spacing_before_wrapper {
        println!();
    }
    let wrapper = if cfg!(target_os = "windows") { "aifo-coder" } else { "./aifo-coder" };
    println!("  {} fork merge --session {} --strategy fetch", wrapper, sid);
    println!("  {} fork merge --session {} --strategy octopus --autoclean", wrapper, sid);
}
