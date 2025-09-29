#[test]
fn test_images_effective_includes_all_agents() {
    // Ensure function is available and includes all expected agents
    let pairs = aifo_coder::commands::images_effective();
    let mut keys = pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
    keys.sort_unstable();

    let mut expected = vec!["aider", "codex", "crush", "opencode", "openhands", "plandex"];
    expected.sort_unstable();

    assert_eq!(keys, expected, "images_effective must list all agents");
    // Basic sanity: all image refs are non-empty and contain a tag separator
    for (_k, v) in pairs {
        assert!(
            !v.is_empty() && v.contains(':'),
            "image ref should be non-empty and contain a tag: {}",
            v
        );
    }
}

#[test]
fn test_images_effective_respects_flavor_env() {
    // Save and restore env var
    let old = std::env::var("AIFO_CODER_IMAGE_FLAVOR").ok();

    // Set slim and check that -slim appears in refs
    std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "slim");
    let pairs = aifo_coder::commands::images_effective();
    for (_k, v) in &pairs {
        assert!(
            v.contains("-slim:"),
            "expected -slim flavor in image ref when env=slim: {}",
            v
        );
    }

    // Restore prior state
    if let Some(val) = old {
        std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", val);
    } else {
        std::env::remove_var("AIFO_CODER_IMAGE_FLAVOR");
    }
}
