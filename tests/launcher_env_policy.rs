#![allow(clippy::module_name_repetitions)]

fn build_preview(agent: &str) -> String {
    // build_docker_preview_only renders a full 'docker run ...' preview string.
    // We use it for invariant checks without requiring Docker.
    aifo_coder::build_docker_preview_only(agent, &[], "node:22-bookworm-slim", None)
}

fn assert_has_env_kv(preview: &str, k: &str, v: &str) {
    // Preview is a shell-escaped docker argv string; env flags are emitted as:
    //   -e 'KEY=VALUE'
    // We check for the specific, stable substring including quotes.
    let needle = format!("-e '{k}={v}'");
    assert!(
        preview.contains(&needle),
        "expected preview to contain env kv: {needle}\npreview:\n{preview}"
    );
}

fn assert_lacks_env_kv(preview: &str, k: &str, v: &str) {
    let needle = format!("-e '{k}={v}'");
    assert!(
        !preview.contains(&needle),
        "did not expect preview to contain env kv: {needle}\npreview:\n{preview}"
    );
}

#[test]
fn launcher_sets_agent_name_and_uniform_path() {
    for agent in ["codex", "crush", "opencode", "letta", "aider", "openhands"] {
        let preview = build_preview(agent);
        assert_has_env_kv(&preview, "AIFO_AGENT_NAME", agent);
        assert_has_env_kv(
            &preview,
            "PATH",
            "/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH",
        );
    }
}

#[test]
fn launcher_sets_smart_toggles_per_agent() {
    // Node agents: smart + node toggle
    for agent in ["codex", "crush", "opencode", "letta"] {
        let preview = build_preview(agent);
        assert_has_env_kv(&preview, "AIFO_SHIM_SMART", "1");
        assert_has_env_kv(&preview, "AIFO_SHIM_SMART_NODE", "1");
        assert_lacks_env_kv(&preview, "AIFO_SHIM_SMART_PYTHON", "1");
    }

    // Python agents: smart + python toggle
    for agent in ["aider", "openhands"] {
        let preview = build_preview(agent);
        assert_has_env_kv(&preview, "AIFO_SHIM_SMART", "1");
        assert_has_env_kv(&preview, "AIFO_SHIM_SMART_PYTHON", "1");
        assert_lacks_env_kv(&preview, "AIFO_SHIM_SMART_NODE", "1");
    }

    // Others should not set tool toggles by default
    for agent in ["plandex"] {
        let preview = build_preview(agent);
        assert_lacks_env_kv(&preview, "AIFO_SHIM_SMART_NODE", "1");
        assert_lacks_env_kv(&preview, "AIFO_SHIM_SMART_PYTHON", "1");
    }
}
