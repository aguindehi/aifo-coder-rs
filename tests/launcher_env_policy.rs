#![allow(clippy::module_name_repetitions)]

fn build_preview(agent: &str) -> String {
    // build_docker_preview_only renders a full 'docker run ...' preview string.
    // We use it for invariant checks without requiring Docker.
    aifo_coder::build_docker_preview_only(agent, &[], "node:22-bookworm-slim", None)
}

#[test]
fn launcher_sets_agent_name_and_uniform_path() {
    for agent in ["codex", "crush", "opencode", "letta", "aider", "openhands"] {
        let preview = build_preview(agent);
        assert!(
            preview.contains("AIFO_AGENT_NAME="),
            "expected AIFO_AGENT_NAME for agent {agent}"
        );
        assert!(
            preview.contains(
                "PATH=/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"
            ),
            "expected shim-first PATH for agent {agent}"
        );
    }
}

#[test]
fn launcher_sets_smart_toggles_per_agent() {
    // Node agents: smart + node toggle
    for agent in ["codex", "crush", "opencode", "letta"] {
        let preview = build_preview(agent);
        assert!(
            preview.contains("AIFO_SHIM_SMART=1"),
            "expected AIFO_SHIM_SMART=1 for node agent {agent}"
        );
        assert!(
            preview.contains("AIFO_SHIM_SMART_NODE=1"),
            "expected AIFO_SHIM_SMART_NODE=1 for node agent {agent}"
        );
        assert!(
            !preview.contains("AIFO_SHIM_SMART_PYTHON=1"),
            "did not expect python toggle for node agent {agent}"
        );
    }

    // Python agents: smart + python toggle
    for agent in ["aider", "openhands"] {
        let preview = build_preview(agent);
        assert!(
            preview.contains("AIFO_SHIM_SMART=1"),
            "expected AIFO_SHIM_SMART=1 for python agent {agent}"
        );
        assert!(
            preview.contains("AIFO_SHIM_SMART_PYTHON=1"),
            "expected AIFO_SHIM_SMART_PYTHON=1 for python agent {agent}"
        );
        assert!(
            !preview.contains("AIFO_SHIM_SMART_NODE=1"),
            "did not expect node toggle for python agent {agent}"
        );
    }

    // Others should not set tool toggles by default
    for agent in ["plandex"] {
        let preview = build_preview(agent);
        assert!(
            !preview.contains("AIFO_SHIM_SMART_NODE=1"),
            "did not expect node toggle for agent {agent}"
        );
        assert!(
            !preview.contains("AIFO_SHIM_SMART_PYTHON=1"),
            "did not expect python toggle for agent {agent}"
        );
    }
}
