#![allow(clippy::module_name_repetitions)]

const SHIM_FIRST_PATH: &str =
    "/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH";

fn build_args(agent: &str) -> Vec<String> {
    // Use the structured argv (not the shell-escaped preview string) for stable assertions.
    //
    // `aifo_coder::docker` is an internal module; public APIs are re-exported at crate root.
    aifo_coder::build_docker_preview_args_only(agent, &[], "node:22-bookworm-slim", None)
}

fn expect_env_kv(args: &[String], key: &str, val: &str) {
    let mut i = 0usize;
    while i + 1 < args.len() {
        if args[i] == "-e" && args[i + 1] == format!("{key}={val}") {
            return;
        }
        i += 1;
    }
    panic!("missing env kv -e {key}={val} in args: {args:?}");
}

fn expect_no_env_kv(args: &[String], key: &str, val: &str) {
    let mut i = 0usize;
    while i + 1 < args.len() {
        if args[i] == "-e" && args[i + 1] == format!("{key}={val}") {
            panic!("unexpected env kv -e {key}={val} in args: {args:?}");
        }
        i += 1;
    }
}

fn container_sh_c_script(args: &[String]) -> &str {
    for i in 0..args.len().saturating_sub(1) {
        if args[i] == "-c" {
            return &args[i + 1];
        }
    }
    panic!("missing /bin/sh -c script in args: {args:?}");
}

#[test]
fn unit_launcher_sets_agent_name_and_uniform_path() {
    for agent in ["codex", "crush", "opencode", "letta", "aider", "openhands"] {
        let args = build_args(agent);
        expect_env_kv(&args, "AIFO_AGENT_NAME", agent);

        let script = container_sh_c_script(&args);
        let expected = format!(r#"export PATH="{SHIM_FIRST_PATH}""#);
        assert!(
            script.contains(&expected),
            "missing shim-first PATH export '{expected}' in container script: {script}"
        );
    }
}

#[test]
fn unit_launcher_sets_smart_toggles_per_agent() {
    // Node agents: smart + node toggle
    for agent in ["codex", "crush", "opencode", "letta"] {
        let args = build_args(agent);
        expect_env_kv(&args, "AIFO_SHIM_SMART", "1");
        expect_env_kv(&args, "AIFO_SHIM_SMART_NODE", "1");
        expect_no_env_kv(&args, "AIFO_SHIM_SMART_PYTHON", "1");
    }

    // Python agents: smart + python toggle
    for agent in ["aider", "openhands"] {
        let args = build_args(agent);
        expect_env_kv(&args, "AIFO_SHIM_SMART", "1");
        expect_env_kv(&args, "AIFO_SHIM_SMART_PYTHON", "1");
        expect_no_env_kv(&args, "AIFO_SHIM_SMART_NODE", "1");
    }

    // Others should not set tool toggles by default
    for agent in ["plandex"] {
        let args = build_args(agent);
        expect_no_env_kv(&args, "AIFO_SHIM_SMART_NODE", "1");
        expect_no_env_kv(&args, "AIFO_SHIM_SMART_PYTHON", "1");
    }
}

#[test]
fn unit_launcher_clears_proxy_env_when_force_direct() {
    use std::env;

    let prev_proxy = env::var("http_proxy").ok();
    env::set_var("http_proxy", "http://bad-proxy");
    aifo_coder::proxy::reset_proxy_state_for_tests();

    // Simulate probe failure then success without proxies to trigger force-direct
    let _ =
        aifo_coder::test_probe_with_proxy_fallback(
            |clear| {
                if clear {
                    Some(true)
                } else {
                    Some(false)
                }
            },
        );

    let args = build_args("codex");
    expect_env_kv(&args, "http_proxy", "");
    expect_env_kv(&args, "https_proxy", "");
    expect_env_kv(&args, "HTTP_PROXY", "");
    expect_env_kv(&args, "HTTPS_PROXY", "");

    match prev_proxy {
        Some(v) => env::set_var("http_proxy", v),
        None => env::remove_var("http_proxy"),
    }
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_launcher_forwards_aifo_env_prefix_and_skips_reserved() {
    use std::env;

    const FORWARD_KEY: &str = "AIFO_ENV_MY_KEY";
    const RESERVED_KEY: &str = "AIFO_ENV_HOME";

    let prev_forward = env::var(FORWARD_KEY).ok();
    let prev_reserved = env::var(RESERVED_KEY).ok();
    env::set_var(FORWARD_KEY, "secret");
    env::set_var(RESERVED_KEY, "/should/not/override");

    let args = build_args("codex");

    // Forwarded without prefix
    expect_env_kv(&args, "MY_KEY", "secret");
    // Reserved keys are ignored to avoid breaking runtime assumptions
    let mut i = 0usize;
    while i + 1 < args.len() {
        if args[i] == "-e" && args[i + 1].starts_with("HOME=") {
            // HOME is set by launcher, but should not be overridden by AIFO_ENV_HOME
            assert_eq!(args[i + 1], "HOME=/home/coder");
        }
        if args[i] == "-e" && args[i + 1] == "HOME=/should/not/override" {
            panic!("AIFO_ENV_HOME should not override HOME");
        }
        i += 1;
    }

    // Restore env
    match prev_forward {
        Some(v) => env::set_var(FORWARD_KEY, v),
        None => env::remove_var(FORWARD_KEY),
    }
    match prev_reserved {
        Some(v) => env::set_var(RESERVED_KEY, v),
        None => env::remove_var(RESERVED_KEY),
    }
}
