#![allow(clippy::module_name_repetitions)]
//! CLI warnings and interactive prompts (tmp workspace, missing toolchains).

use std::io::Write;

pub fn warn_if_tmp_workspace(interactive_block: bool) -> bool {
    if std::env::var("AIFO_CODER_SUPPRESS_TMP_WARNING")
        .ok()
        .as_deref()
        == Some("1")
    {
        return true;
    }
    let pwd = match std::env::current_dir() {
        Ok(p) => std::fs::canonicalize(&p).unwrap_or(p),
        Err(_) => return true,
    };
    let s = pwd.display().to_string();

    if cfg!(target_os = "macos") {
        if s == "/tmp"
            || s.starts_with("/tmp/")
            || s == "/private/tmp"
            || s.starts_with("/private/tmp/")
            || s.starts_with("/private/var/folders/")
        {
            let mut msgs: Vec<String> = Vec::new();
            msgs.push(format!(
                "current workspace is under a temporary path ({}).",
                s
            ));
            msgs.push("on macOS, /tmp is a symlink to /private/tmp and many /private/var/folders/* paths are not shared with Docker Desktop by default.".to_string());
            msgs.push(
                "this can result in an empty or non-writable /workspace inside the container."
                    .to_string(),
            );
            msgs.push(
                "move your project under your home directory (e.g., ~/projects/<repo>) and retry."
                    .to_string(),
            );
            if interactive_block {
                let lines: Vec<&str> = msgs.iter().map(|m| m.as_str()).collect();
                return aifo_coder::warn_prompt_continue_or_quit(&lines);
            } else {
                for m in msgs {
                    aifo_coder::warn_print(&m);
                }
            }
        }
    } else if s == "/tmp" || s.starts_with("/tmp/") || s == "/var/tmp" || s.starts_with("/var/tmp/")
    {
        let mut msgs: Vec<String> = Vec::new();
        msgs.push(format!(
            "current workspace is under a temporary path ({}).",
            s
        ));
        msgs.push(
            "some Docker setups do not share temporary folders reliably with containers."
                .to_string(),
        );
        msgs.push("you may see an empty or read-only /workspace. move the project under your home directory and retry.".to_string());
        if interactive_block {
            let lines: Vec<&str> = msgs.iter().map(|m| m.as_str()).collect();
            return aifo_coder::warn_prompt_continue_or_quit(&lines);
        } else {
            for m in msgs {
                aifo_coder::warn_print(&m);
            }
        }
    }
    true
}

// Warn at startup (agent-run path) when no toolchains are requested and no proxy is configured.
pub fn maybe_warn_missing_toolchain_agent(cli: &crate::cli::Cli, agent: &str) {
    // Respect explicit suppression
    if std::env::var("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING")
        .ok()
        .as_deref()
        == Some("1")
    {
        return;
    }
    // Only warn for interactive agent runs
    if agent != "aider" && agent != "crush" && agent != "codex" {
        return;
    }
    if !cli.toolchain.is_empty() || !cli.toolchain_spec.is_empty() {
        return;
    }
    let has_url = std::env::var("AIFO_TOOLEEXEC_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_tok = std::env::var("AIFO_TOOLEEXEC_TOKEN")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if has_url && has_tok {
        return;
    }
    // Emit concise guidance to stderr (color-aware)
    let use_err = aifo_coder::color_enabled_stderr();
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33;1m",
            "warning: no language toolchain sidecars enabled (--toolchain)."
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "without toolchains, PATH shims (cargo, rustc, node, npm, tsc, python, pip, gcc/clang, go, …) will not be proxied and builds may fail."
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "enable toolchains as needed, e.g.: aifo-coder --toolchain rust --toolchain node --toolchain python aider --"
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "pin versions: --toolchain-spec rust@1.80 --toolchain-spec node@22 --toolchain-spec python@3.12"
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "options: --toolchain-image kind=image, --no-toolchain-cache, and on Linux --toolchain-unix-socket"
        )
    );
}

// Fork orchestrator preflight warning with single continue/abort prompt.
pub fn maybe_warn_missing_toolchain_for_fork(cli: &crate::cli::Cli, agent: &str) -> bool {
    // Respect explicit suppression
    if std::env::var("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING")
        .ok()
        .as_deref()
        == Some("1")
    {
        return true;
    }
    // Only warn for coding agents
    if agent != "aider" && agent != "crush" && agent != "codex" {
        return true;
    }
    // No toolchain flags?
    if !cli.toolchain.is_empty() || !cli.toolchain_spec.is_empty() {
        return true;
    }
    // If proxy already configured, don't warn
    let has_url = std::env::var("AIFO_TOOLEEXEC_URL")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let has_tok = std::env::var("AIFO_TOOLEEXEC_TOKEN")
        .ok()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if has_url && has_tok {
        return true;
    }
    // Build lines and prompt once
    let msgs: Vec<String> = vec![
        "no language toolchain sidecars enabled (--toolchain).".to_string(),
        "without toolchains, PATH shims (cargo, rustc, node, npm, tsc, python, pip, gcc/clang, go, …) will not be proxied and builds may fail.".to_string(),
        "enable toolchains as needed, e.g.: aifo-coder --toolchain rust --toolchain node --toolchain python aider --".to_string(),
        "pin versions: --toolchain-spec rust@1.80 --toolchain-spec node@22 --toolchain-spec python@3.12".to_string(),
        "options: --toolchain-image kind=image, --no-toolchain-cache, and on Linux --toolchain-unix-socket".to_string(),
    ];
    let lines: Vec<&str> = msgs.iter().map(|m| m.as_str()).collect();
    aifo_coder::warn_prompt_continue_or_quit(&lines)
}
