/*
ignore-tidy-linelength
*/

use std::process::Command;

fn find_bin() -> Option<String> {
    if let Some(p) = option_env!("CARGO_BIN_EXE_aifo_coder") {
        return Some(p.to_string());
    }
    None
}

#[test]
fn int_images_lists_all_agents_with_slim_flavor() {
    let exe = match find_bin() {
        Some(p) => p,
        None => {
            eprintln!("skipping: CARGO_BIN_EXE_aifo_coder not available");
            return;
        }
    };
    // Force deterministic output: slim flavor and no registry prefix
    std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "slim");
    std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", "");

    let out = Command::new(&exe)
        .args(["images", "--color=never"])
        .output()
        .expect("run aifo-coder images");
    assert!(
        out.status.success(),
        "images exited non-zero; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    for agent in [
        "codex",
        "crush",
        "aider",
        "openhands",
        "opencode",
        "plandex",
    ] {
        let line = stdout
            .lines()
            .find(|l| l.starts_with(&format!("{agent} ")))
            .unwrap_or("");
        assert!(
            !line.is_empty(),
            "missing stdout line for agent {agent}; got:\n{}",
            stdout
        );
        let img = line.split_whitespace().nth(1).unwrap_or("");
        assert!(
            img.contains("-slim:"),
            "expected slim image (suffix -slim) in '{}'",
            img
        );
    }
}

#[test]
fn int_images_respects_registry_env_override() {
    let exe = match find_bin() {
        Some(p) => p,
        None => {
            eprintln!("skipping: CARGO_BIN_EXE_aifo_coder not available");
            return;
        }
    };
    // Force deterministic registry prefix and full flavor
    std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "full");
    std::env::set_var("AIFO_CODER_REGISTRY_PREFIX", "example.com////");

    let out = Command::new(&exe)
        .args(["images", "--color=never"])
        .output()
        .expect("run aifo-coder images");
    assert!(
        out.status.success(),
        "images exited non-zero; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    for agent in [
        "codex",
        "crush",
        "aider",
        "openhands",
        "opencode",
        "plandex",
    ] {
        let line = stdout
            .lines()
            .find(|l| l.starts_with(&format!("{agent} ")))
            .unwrap_or("");
        let img = line.split_whitespace().nth(1).unwrap_or("");
        assert!(
            img.starts_with("example.com/"),
            "expected registry prefix example.com/ in '{}'",
            img
        );
        assert!(
            img.contains(&format!("aifo-coder-{agent}:")),
            "expected image name aifo-coder-{agent}: in '{}'",
            img
        );
        assert!(
            !img.contains("-slim:"),
            "expected full flavor (no -slim) in '{}'",
            img
        );
    }
}
