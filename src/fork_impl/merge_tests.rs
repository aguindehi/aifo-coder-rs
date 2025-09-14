#[cfg(test)]
mod tests {
    use super::super::fork_impl_merge::compose_merge_message_impl;
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
    fn test_compose_merge_message_prefix_and_truncation() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with main and feature
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();

        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Create feature branch and add a very long subject line to trigger truncation
        assert!(Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let long_subject = "feat: ".to_string() + &"x".repeat(200);
        std::fs::write(repo.join("b.txt"), "b\n").unwrap();
        assert!(Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args(["commit", "-m", &long_subject])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Compose message relative to main
        let msg =
            compose_merge_message_impl(repo, &[(".".into(), "feature".to_string())], "main");

        // First line must start with "Octopus merge:"
        let first_line = msg.lines().next().unwrap_or("");
        assert!(
            first_line.to_ascii_lowercase().starts_with("octopus merge"),
            "first line should start with 'Octopus merge', got: {}",
            first_line
        );

        // Ensure truncation occurred (summary line length <= ~170)
        assert!(
            first_line.len() <= 170,
            "summary line should be reasonably truncated, got len={} line={}",
            first_line.len(),
            first_line
        );
    }
}
