#[test]
fn test_images_effective_includes_all_agents() {
    // Compose expected refs using environment and preferred registry prefix
    let prefix =
        std::env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = std::env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let flavor = std::env::var("AIFO_CODER_IMAGE_FLAVOR").ok();
    let suffix = if flavor
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("slim"))
        .unwrap_or(false)
    {
        "-slim"
    } else {
        ""
    };
    let reg = aifo_coder::preferred_registry_prefix_quiet();
    let agents = vec![
        "aider",
        "codex",
        "crush",
        "opencode",
        "openhands",
        "plandex",
    ];
    let mut pairs = Vec::new();
    for a in &agents {
        let name = format!("{prefix}-{a}{suffix}:{tag}");
        let full = if reg.is_empty() {
            name
        } else {
            format!("{reg}{name}")
        };
        pairs.push((a.to_string(), full));
    }

    let mut keys = pairs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
    keys.sort_unstable();

    let mut expected = vec![
        "aider",
        "codex",
        "crush",
        "opencode",
        "openhands",
        "plandex",
    ];
    expected.sort_unstable();

    assert_eq!(keys, expected, "must list all agents");
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

    let prefix =
        std::env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = std::env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let reg = aifo_coder::preferred_registry_prefix_quiet();
    let agents = vec![
        "aider",
        "codex",
        "crush",
        "opencode",
        "openhands",
        "plandex",
    ];

    for a in &agents {
        let name = format!("{prefix}-{a}-slim:{tag}");
        let full = if reg.is_empty() {
            name.clone()
        } else {
            format!("{reg}{name}")
        };
        assert!(
            full.contains("-slim:"),
            "expected -slim flavor in image ref when env=slim: {}",
            full
        );
    }

    // Restore prior state
    if let Some(val) = old {
        std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", val);
    } else {
        std::env::remove_var("AIFO_CODER_IMAGE_FLAVOR");
    }
}
