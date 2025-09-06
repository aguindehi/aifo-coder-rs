use aifo_coder::{
    acquire_lock, build_docker_cmd, desired_apparmor_profile, preferred_registry_prefix,
};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use which::which;

fn print_startup_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(
        " 🚀  Welcome to the Migros AI Foundation Coder - AIFO Coder v{}  🚀 ",
        version
    );
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(
        " 🔒 Secure by Design | 🌍 Cross-Platform | 🦀 Powered by Rust | 🧠 Developed by AIFO"
    );
    println!();

    // Host/platform info
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // Virtualization environment (terse)
    let virtualization = if cfg!(target_os = "macos") {
        match std::process::Command::new("colima")
            .arg("status")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
        {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if s.contains("running") {
                    "Colima VM"
                } else {
                    "Docker Desktop/VM"
                }
            }
            Err(_) => "Docker Desktop/VM",
        }
    } else if cfg!(target_os = "windows") {
        "Docker Desktop/VM"
    } else {
        "native"
    };

    // Docker runtime path (terse)
    let docker_disp = aifo_coder::container_runtime_path()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(not found)".to_string());

    // Security options (seccomp/cgroupns/rootless) and AppArmor status
    let apparmor_supported = aifo_coder::docker_supports_apparmor();
    let apparmor_profile = aifo_coder::desired_apparmor_profile_quiet();
    let (mut seccomp, mut cgroupns, mut rootless) =
        ("(unknown)".to_string(), "(unknown)".to_string(), false);
    if let Ok(rt) = aifo_coder::container_runtime_path() {
        if let Ok(out) = std::process::Command::new(&rt)
            .args(["info", "--format", "{{json .SecurityOptions}}"])
            .output()
        {
            let raw = String::from_utf8_lossy(&out.stdout);
            // Extract quoted items from JSON array of strings
            let mut items: Vec<String> = Vec::new();
            let mut in_str = false;
            let mut esc = false;
            let mut buf = String::new();
            for ch in raw.chars() {
                if in_str {
                    if esc {
                        buf.push(ch);
                        esc = false;
                    } else if ch == '\\' {
                        esc = true;
                    } else if ch == '"' {
                        items.push(buf.clone());
                        buf.clear();
                        in_str = false;
                    } else {
                        buf.push(ch);
                    }
                } else if ch == '"' {
                    in_str = true;
                }
            }
            for s in &items {
                if s.contains("name=seccomp") {
                    for part in s.split(',') {
                        if let Some(v) = part.strip_prefix("profile=") {
                            seccomp = v.to_string();
                            break;
                        }
                    }
                } else if s.contains("name=cgroupns") {
                    for part in s.split(',') {
                        if let Some(v) = part.strip_prefix("mode=") {
                            cgroupns = v.to_string();
                            break;
                        }
                    }
                } else if s.contains("rootless") {
                    rootless = true;
                }
            }
        }
    }

    // Feature overview (Linux, macOS, Windows)
    println!(" ✨ Features:");
    println!("    - Linux: Docker containers with AppArmor when available; seccomp and cgroup namespaces.");
    println!(
        "    - macOS: Docker Desktop/Colima VM isolation; same security features inside the VM."
    );
    println!("    - Windows: Docker Desktop VM; Windows Terminal/PowerShell/Git Bash fork orchestration.");
    println!();

    // Dynamic startup summary (terse)
    println!(" ⚙️  Starting up coding agents...");
    println!(
        "    - Environment: Docker={} | Virt={}",
        docker_disp, virtualization
    );
    println!("    - Platform: {}/{}", os, arch);
    let aa = if apparmor_supported {
        match apparmor_profile.as_deref() {
            Some(p) => format!("AppArmor=on ({})", p),
            None => "AppArmor=on".to_string(),
        }
    } else {
        "AppArmor=off".to_string()
    };
    println!(
        "    - Security: {}, Seccomp={}, cgroupns={}, rootless={}",
        aa,
        seccomp,
        cgroupns,
        if rootless { "yes" } else { "no" }
    );
    println!("    - Version: {}", version);
    println!();

    // Safety highlights (concise, current capabilities)
    println!(" 🔧 Building a safer future for coding automation in Migros Group...");
    println!("    - Containerized agents; no privileged mode, no host Docker socket.");
    println!("    - AppArmor (Linux) with custom 'aifo-coder' or 'docker-default' when available.");
    println!("    - Seccomp and cgroup namespaces as reported by Docker.");
    println!("    - Per-pane isolated state for forks (.aider/.codex/.crush).");
    println!(
        "    - Language toolchain sidecars (rust, node/ts, python, c/cpp, go) via secure proxy."
    );
    println!("    - Optional unix:// proxy on Linux; host-gateway bridging when needed.");
    println!("    - Minimal mounts: project workspace, config files, optional GnuPG keyrings.");
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 📜 Written 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of Migros AI Foundation at MGB");
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!();
}

fn warn_if_tmp_workspace(interactive_block: bool) -> bool {
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
                "Current workspace is under a temporary path ({}).",
                s
            ));
            msgs.push("On macOS, /tmp is a symlink to /private/tmp and many /private/var/folders/* paths are not shared with Docker Desktop by default.".to_string());
            msgs.push(
                "This can result in an empty or non-writable /workspace inside the container."
                    .to_string(),
            );
            msgs.push(
                "Move your project under your home directory (e.g., ~/projects/<repo>) and retry."
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
    } else {
        if s == "/tmp" || s.starts_with("/tmp/") || s == "/var/tmp" || s.starts_with("/var/tmp/") {
            let mut msgs: Vec<String> = Vec::new();
            msgs.push(format!(
                "Current workspace is under a temporary path ({}).",
                s
            ));
            msgs.push(
                "Some Docker setups do not share temporary folders reliably with containers."
                    .to_string(),
            );
            msgs.push("You may see an empty or read-only /workspace. Move the project under your home directory and retry.".to_string());
            if interactive_block {
                let lines: Vec<&str> = msgs.iter().map(|m| m.as_str()).collect();
                return aifo_coder::warn_prompt_continue_or_quit(&lines);
            } else {
                for m in msgs {
                    aifo_coder::warn_print(&m);
                }
            }
        }
    }
    true
}

// Warn at startup (agent-run path) when no toolchains are requested and no proxy is configured.
fn maybe_warn_missing_toolchain_agent(cli: &Cli, agent: &str) {
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
            "Without toolchains, PATH shims (cargo, rustc, node, npm, tsc, python, pip, gcc/clang, go, …) will not be proxied and builds may fail."
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "Enable toolchains as needed, e.g.: aifo-coder --toolchain rust --toolchain node --toolchain python aider --"
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "Pin versions: --toolchain-spec rust@1.80 --toolchain-spec node@22 --toolchain-spec python@3.12"
        )
    );
    eprintln!(
        "{}",
        aifo_coder::paint(
            use_err,
            "\x1b[33m",
            "Options: --toolchain-image kind=image, --no-toolchain-cache, and on Linux --toolchain-unix-socket"
        )
    );
}

// Fork orchestrator preflight warning with single continue/abort prompt.
fn maybe_warn_missing_toolchain_for_fork(cli: &Cli, agent: &str) -> bool {
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
    let mut msgs: Vec<String> = Vec::new();
    msgs.push("no language toolchain sidecars enabled (--toolchain).".to_string());
    msgs.push("Without toolchains, PATH shims (cargo, rustc, node, npm, tsc, python, pip, gcc/clang, go, …) will not be proxied and builds may fail.".to_string());
    msgs.push("Enable toolchains as needed, e.g.: aifo-coder --toolchain rust --toolchain node --toolchain python aider --".to_string());
    msgs.push("Pin versions: --toolchain-spec rust@1.80 --toolchain-spec node@22 --toolchain-spec python@3.12".to_string());
    msgs.push("Options: --toolchain-image kind=image, --no-toolchain-cache, and on Linux --toolchain-unix-socket".to_string());
    let lines: Vec<&str> = msgs.iter().map(|m| m.as_str()).collect();
    aifo_coder::warn_prompt_continue_or_quit(&lines)
}

#[cfg(test)]
mod tests_main_cli_child_args {

    fn make_cli_for_test() -> super::Cli {
        super::Cli {
            image: Some("example.com/org/agent:tag".to_string()),
            toolchain: vec![super::ToolchainKind::Rust, super::ToolchainKind::Node],
            toolchain_spec: vec!["python@3.12".to_string()],
            toolchain_image: vec!["go=golang:1.22-bookworm".to_string()],
            no_toolchain_cache: true,
            toolchain_unix_socket: false,
            toolchain_bootstrap: vec!["typescript=global".to_string()],
            verbose: true,
            flavor: Some(super::Flavor::Slim),
            invalidate_registry_cache: false,
            dry_run: true,
            // fork flags that must be stripped from child args
            fork: Some(3),
            fork_include_dirty: true,
            fork_dissociate: true,
            fork_session_name: Some("ut-session".to_string()),
            fork_layout: Some("even-h".to_string()),
            fork_keep_on_failure: true,
            fork_merging_strategy: aifo_coder::MergingStrategy::None,
            fork_merging_autoclean: false,
            color: Some(aifo_coder::ColorMode::Auto),
            command: super::Agent::Aider {
                args: vec!["--help".to_string(), "--".to_string(), "extra".to_string()],
            },
        }
    }

    #[test]
    fn test_fork_build_child_args_strips_fork_flags_and_preserves_agent_args() {
        let cli = make_cli_for_test();
        let args = super::fork_build_child_args(&cli);
        // Must contain agent subcommand (root flags precede it)
        assert!(
            args.iter().any(|s| s == "aider"),
            "child args must contain agent subcommand, got: {:?}",
            args
        );
        // Must include some global flags we set
        let joined = args.join(" ");
        assert!(
            joined.contains("--image example.com/org/agent:tag"),
            "expected --image in child args: {}",
            joined
        );
        assert!(
            joined.contains("--toolchain rust") && joined.contains("--toolchain node"),
            "expected --toolchain flags in child args: {}",
            joined
        );
        assert!(
            joined.contains("--toolchain-spec python@3.12"),
            "expected --toolchain-spec in child args: {}",
            joined
        );
        assert!(
            joined.contains("--toolchain-image go=golang:1.22-bookworm"),
            "expected --toolchain-image in child args: {}",
            joined
        );
        assert!(
            joined.contains("--no-toolchain-cache"),
            "expected --no-toolchain-cache in child args: {}",
            joined
        );
        assert!(
            joined.contains("--flavor slim"),
            "expected --flavor slim in child args: {}",
            joined
        );
        assert!(
            joined.contains("--dry-run"),
            "expected --dry-run in child args: {}",
            joined
        );
        // Must NOT contain any fork flags
        for bad in [
            "--fork ",
            "--fork-include-dirty",
            "--fork-dissociate",
            "--fork-session-name",
            "--fork-layout",
            "--fork-keep-on-failure",
            "--fork-merge-strategy",
            "--fork-merge-no-autoclean",
        ] {
            assert!(
                !joined.contains(bad),
                "fork flag leaked into child args: {} in {}",
                bad,
                joined
            );
        }
        // Agent args should be preserved; literal '--' boundary must be present and ordering intact
        assert!(
            args.contains(&"--".to_string()),
            "expected literal '--' argument to be present in child args: {:?}",
            args
        );
        let idx_boundary = args.iter().position(|s| s == "--");
        let idx_extra = args.iter().position(|s| s == "extra");
        assert!(
            idx_boundary.is_some() && idx_extra.is_some() && idx_boundary < idx_extra,
            "expected '--' to appear before trailing agent args, got: {:?}",
            args
        );
        let dashdash_count = args.iter().filter(|s| s.as_str() == "--").count();
        assert_eq!(
            dashdash_count, 1,
            "expected exactly one '--' token, got: {:?}",
            args
        );
        assert!(
            joined.contains("aider --help"),
            "expected agent arg '--help' to be present: {}",
            joined
        );
        assert!(
            joined.contains("extra"),
            "expected trailing agent arg to be present: {}",
            joined
        );
    }

    #[test]
    fn test_merging_strategy_value_enum_parsing() {
        use clap::Parser;
        let cli = super::Cli::parse_from([
            "aifo-coder",
            "--fork-merge-strategy",
            "octopus",
            "aider",
            "--",
            "--help",
        ]);
        assert!(
            matches!(
                cli.fork_merging_strategy,
                aifo_coder::MergingStrategy::Octopus
            ),
            "expected parsing of --fork-merge-strategy octopus"
        );
    }
}

fn run_doctor(verbose: bool) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!("aifo-coder doctor");
    eprintln!();
    eprintln!("  version: v{}", version);
    eprintln!(
        "  host:    {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    eprintln!();

    // Virtualization environment
    let virtualization = if cfg!(target_os = "macos") {
        match Command::new("colima")
            .arg("status")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
        {
            Ok(out) => {
                let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
                if s.contains("running") {
                    "Colima VM"
                } else {
                    "Docker Desktop or other"
                }
            }
            Err(_) => "Docker Desktop or other",
        }
    } else {
        "native"
    };
    eprintln!("  virtualization: {}", virtualization);
    eprintln!();

    // Docker/AppArmor capabilities
    let apparmor_supported = aifo_coder::docker_supports_apparmor();
    let das = if apparmor_supported { "yes" } else { "no" };
    let das_val = if atty::is(atty::Stream::Stderr) {
        format!("\x1b[34;1m{}\x1b[0m", das)
    } else {
        das.to_string()
    };
    eprintln!("  docker apparmor support: {}", das_val);

    // Parse and display Docker security options (from `docker info`)
    if let Ok(rt) = aifo_coder::container_runtime_path() {
        if let Ok(out) = Command::new(&rt)
            .args(["info", "--format", "{{json .SecurityOptions}}"])
            .output()
        {
            let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Extract JSON string array items without external deps
            let mut items: Vec<String> = Vec::new();
            let mut in_str = false;
            let mut esc = false;
            let mut buf = String::new();
            for ch in raw.chars() {
                if in_str {
                    if esc {
                        buf.push(ch);
                        esc = false;
                    } else if ch == '\\' {
                        esc = true;
                    } else if ch == '"' {
                        items.push(buf.clone());
                        buf.clear();
                        in_str = false;
                    } else {
                        buf.push(ch);
                    }
                } else if ch == '"' {
                    in_str = true;
                }
            }
            let pretty: Vec<String> = items
                .iter()
                .cloned()
                .map(|s| {
                    let mut name: Option<String> = None;
                    let mut attrs: Vec<String> = Vec::new();
                    for part in s.split(',') {
                        if let Some(v) = part.strip_prefix("name=") {
                            name = Some(v.to_string());
                        } else {
                            attrs.push(part.to_string());
                        }
                    }
                    match name {
                        Some(n) => {
                            if attrs.is_empty() {
                                n
                            } else {
                                format!("{} ({})", n, attrs.join(", "))
                            }
                        }
                        None => s,
                    }
                })
                .collect();
            let joined = if pretty.is_empty() {
                "(none)".to_string()
            } else {
                pretty.join(", ")
            };
            let joined_val = joined.clone();
            eprintln!("  docker security options: {}", joined_val);
            {
                let has_apparmor = items.iter().any(|s| s.contains("apparmor"));
                // Extract seccomp profile if present
                let mut seccomp = String::from("(unknown)");
                for s in &items {
                    if s.contains("name=seccomp") {
                        for part in s.split(',') {
                            if let Some(v) = part.strip_prefix("profile=") {
                                seccomp = v.to_string();
                                break;
                            }
                        }
                        break;
                    }
                }
                // Extract cgroupns mode if present
                let mut cgroupns = String::from("(unknown)");
                for s in &items {
                    if s.contains("name=cgroupns") {
                        for part in s.split(',') {
                            if let Some(v) = part.strip_prefix("mode=") {
                                cgroupns = v.to_string();
                                break;
                            }
                        }
                        break;
                    }
                }
                let rootless = items.iter().any(|s| s.contains("rootless"));
                eprintln!(
                    "  docker security details: AppArmor={}, Seccomp={}, cgroupns={}, rootless={}",
                    if has_apparmor { "yes" } else { "no" },
                    seccomp,
                    cgroupns,
                    if rootless { "yes" } else { "no" }
                );
            }
            if verbose {
                let has_apparmor = items.iter().any(|s| s.contains("apparmor"));
                // Extract seccomp profile if present
                let mut seccomp = String::from("(unknown)");
                for s in &items {
                    if s.contains("name=seccomp") {
                        for part in s.split(',') {
                            if let Some(v) = part.strip_prefix("profile=") {
                                seccomp = v.to_string();
                                break;
                            }
                        }
                        break;
                    }
                }

                // security details were printed above in non-verbose section; only show tips here
                if !has_apparmor {
                    eprintln!("    tip: AppArmor not reported by Docker. On Linux, enable the AppArmor kernel module and ensure Docker is built with AppArmor support.");
                }
                if seccomp.eq_ignore_ascii_case("unconfined") {
                    eprintln!("    tip: Docker daemon seccomp profile is 'unconfined'. Consider switching to the default seccomp profile for better isolation.");
                }
            }
        }
    }
    eprintln!();

    // Desired AppArmor profile
    let profile = aifo_coder::desired_apparmor_profile_quiet();
    let prof_str = profile.as_deref().unwrap_or("(disabled)");
    eprintln!("  apparmor profile:      {}", prof_str);

    // Confirm active AppArmor profile from inside a short-lived container
    if aifo_coder::container_runtime_path().is_ok() {
        let image = default_image_for_quiet("crush");
        let mut args = vec!["run".to_string(), "--rm".to_string()];
        if aifo_coder::docker_supports_apparmor() {
            if let Some(p) = profile.as_deref() {
                args.push("--security-opt".to_string());
                args.push(format!("apparmor={}", p));
            }
        }
        args.push("--entrypoint".to_string());
        args.push("sh".to_string());
        args.push(image);
        args.push("-lc".to_string());
        args.push(
            "cat /proc/self/attr/apparmor/current 2>/dev/null || echo unconfined".to_string(),
        );
        let mut cmd = Command::new("docker");
        for a in &args {
            cmd.arg(a);
        }
        let current = cmd
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "(unknown)".to_string());
        let current_trim = current.trim().to_string();
        eprintln!("  apparmor in-container: {}", current_trim);

        // Validate AppArmor status against expectations
        let expected = profile.as_deref();
        let expected_disp = expected.unwrap_or("(none)");

        let status_plain = {
            if !apparmor_supported {
                "skipped".to_string()
            } else if current_trim == "(unknown)" || current_trim.is_empty() {
                "unknown".to_string()
            } else if current_trim == "unconfined" {
                "FAIL".to_string()
            } else if let Some(p) = expected {
                if current_trim.starts_with(p) {
                    "PASS".to_string()
                } else {
                    "WARN".to_string()
                }
            } else {
                "PASS".to_string()
            }
        };
        eprintln!(
            "  apparmor validation:   {} (expected: {})",
            status_plain, expected_disp
        );
        if verbose {
            match status_plain.as_str() {
                "FAIL" => {
                    if cfg!(target_os = "linux") {
                        eprintln!(
                            "    tip: Container is unconfined. Generate and load the profile:"
                        );
                        eprintln!("    tip:   make apparmor");
                        eprintln!(
                            "    tip:   sudo apparmor_parser -r -W \"build/apparmor/aifo-coder\""
                        );
                        eprintln!("    tip: Then re-run with AppArmor enabled.");
                    } else {
                        eprintln!("    tip: Container appears unconfined. Ensure your Docker VM/distribution supports AppArmor and it is enabled.");
                    }
                }
                "WARN" => {
                    eprintln!("    tip: Active AppArmor profile differs from expected. If you set AIFO_CODER_APPARMOR_PROFILE, verify the profile is loaded on the host ('/sys/kernel/security/apparmor/profiles').");
                }
                "unknown" => {
                    eprintln!("    tip: Unable to read AppArmor status from container. Ensure 'docker run' works and that /proc/self/attr/apparmor/current is accessible.");
                }
                _ => {}
            }
        }
    }
    eprintln!();

    // Docker command and version
    match aifo_coder::container_runtime_path() {
        Ok(p) => {
            eprintln!("  docker command:  {}", p.display());
            if let Ok(out) = Command::new(&p).arg("--version").output() {
                let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
                // Typical: "Docker version 28.3.3, build 980b856816"
                let pretty = raw.trim_start_matches("Docker version ").to_string();
                eprintln!("  docker version:  {}", pretty);
            }
        }
        Err(_) => {
            eprintln!("  docker command:  (not found)");
            if verbose {
                eprintln!("    tip: Install Docker and ensure 'docker' is in your PATH. On Linux, install Docker Engine; on macOS, install Docker Desktop or use Colima.");
            }
        }
    }

    // Registry (quiet probe; no intermediate logs)
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let reg_display = if rp.is_empty() {
        "Docker Hub".to_string()
    } else {
        rp.trim_end_matches('/').to_string()
    };
    eprintln!("  docker registry: {}", reg_display);
    // (registry source suppressed)
    eprintln!();

    // Print stale fork sessions notice during doctor runs (Phase 6)
    aifo_coder::fork_print_stale_notice();
    eprintln!();

    // Helpful config/state locations (display with ~)
    let home = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("~"));
    let home_str = home.to_string_lossy().to_string();
    let show = |label: &str, path: std::path::PathBuf, _mounted: bool| {
        let pstr = path.display().to_string();
        let shown = if pstr.starts_with(&home_str) {
            format!("~{}", &pstr[home_str.len()..])
        } else {
            pstr
        };
        let exists = path.exists();
        let use_color = atty::is(atty::Stream::Stderr);

        // Column widths
        let label_width: usize = 16;
        let path_col: usize = 44; // target visible width for path column (moved left)
        let _status_col: usize = 14; // deprecated: second status column removed

        // Compute visible width before building colored_path to avoid moving 'shown' prematurely.
        let visible_len = shown.chars().count();
        let pad_spaces = if visible_len < path_col {
            path_col - visible_len
        } else {
            1
        };
        let padding = " ".repeat(pad_spaces);

        // Colorize the path itself as a value (strong blue)
        let colored_path = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", shown) // strong blue
        } else {
            shown
        };

        // Build status cells (plain)
        let (icon1, text1) = if exists {
            ("✅", "found")
        } else {
            ("❌", "missing")
        };
        let cell1_plain = format!("{} {}", icon1, text1);

        // Colorize status
        let colored_cell1 = if use_color {
            if exists {
                format!("\x1b[32m{}\x1b[0m", cell1_plain)
            } else {
                format!("\x1b[31m{}\x1b[0m", cell1_plain)
            }
        } else {
            cell1_plain.clone()
        };

        eprintln!(
            "  {:label_width$} {}{} {}",
            label,
            colored_path,
            padding,
            colored_cell1,
            label_width = label_width
        );
    };

    // Editor availability for installed images (full and/or slim) via crush image
    if aifo_coder::container_runtime_path().is_ok() {
        let prefix =
            std::env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
        let tag = std::env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
        let candidates = vec![
            ("full", format!("{}-crush:{}", prefix, tag)),
            ("slim", format!("{}-crush-slim:{}", prefix, tag)),
        ];
        let check = "for e in emacs-nox vim nano mg nvi; do command -v \"$e\" >/dev/null 2>&1 && printf \"%s \" \"$e\"; done";
        let use_color = atty::is(atty::Stream::Stderr);
        let mut printed_any = false;

        for (label, img) in candidates {
            // Show only for locally present images; avoid pulling during doctor
            let present = Command::new("docker")
                .args(["image", "inspect", &img])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !present {
                continue;
            }

            if let Ok(out) = Command::new("docker")
                .args(["run", "--rm", "--entrypoint", "sh", &img, "-lc", check])
                .output()
            {
                let list = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let show = if list.is_empty() {
                    "(none)".to_string()
                } else {
                    list
                };
                let val = if use_color {
                    format!("\x1b[34;1m{}\x1b[0m", show)
                } else {
                    show
                };
                eprintln!("  editors ({}):  {}", label, val);
                printed_any = true;
            }
        }

        // Fallback: if neither full nor slim is installed locally, show the default image result once
        if !printed_any {
            let image = default_image_for_quiet("crush");
            if let Ok(out) = Command::new("docker")
                .args(["run", "--rm", "--entrypoint", "sh", &image, "-lc", check])
                .output()
            {
                let list = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let show = if list.is_empty() {
                    "(none)".to_string()
                } else {
                    list
                };
                let val = if use_color {
                    format!("\x1b[34;1m{}\x1b[0m", show)
                } else {
                    show
                };
                eprintln!("  editors:        {}", val);
            }
        }
    }
    eprintln!();

    // Local time and timezone from host (mounted only if present)
    show(
        "local time:",
        std::path::PathBuf::from("/etc/timezone"),
        std::path::Path::new("/etc/timezone").exists(),
    );
    show(
        "local timezone:",
        std::path::PathBuf::from("/etc/localtime"),
        std::path::Path::new("/etc/localtime").exists(),
    );
    eprintln!();

    // Git and GnuPG
    let agent_ctx =
        std::env::var("AIFO_CODER_DOCTOR_AGENT").unwrap_or_else(|_| "aider".to_string());
    let mount_git = true;
    let mount_gnupg = true;
    let mount_aider = agent_ctx.eq_ignore_ascii_case("aider");
    let mount_crush = agent_ctx.eq_ignore_ascii_case("crush");
    let mount_codex = agent_ctx.eq_ignore_ascii_case("codex");

    show("git config:", home.join(".gitconfig"), mount_git);
    show("gnupg config:", home.join(".gnupg"), mount_gnupg);
    eprintln!();

    // Aider files
    show("aider config:", home.join(".aider.conf.yml"), mount_aider);
    show(
        "aider metadata:",
        home.join(".aider.model.metadata.json"),
        mount_aider,
    );
    show(
        "aider settings:",
        home.join(".aider.model.settings.yml"),
        mount_aider,
    );
    eprintln!();

    // Crush paths
    show(
        "crush config:",
        home.join(".local").join("share").join("crush"),
        mount_crush,
    );
    show("crush state:", home.join(".crush"), mount_crush);
    eprintln!();

    // Codex path
    show("codex config:", home.join(".codex"), mount_codex);
    eprintln!();

    // AIFO API environment variables availability
    {
        let use_color = atty::is(atty::Stream::Stderr);
        let icon = |present: bool| -> String {
            if present {
                if use_color {
                    "\x1b[32m✅ found\x1b[0m".to_string()
                } else {
                    "✅ found".to_string()
                }
            } else {
                if use_color {
                    "\x1b[31m❌ missing\x1b[0m".to_string()
                } else {
                    "❌ missing".to_string()
                }
            }
        };
        let present = |name: &str| {
            std::env::var(name)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        };
        let has_key = present("AIFO_API_KEY");
        let has_base = present("AIFO_API_BASE");
        let has_version = present("AIFO_API_VERSION");

        let label_w: usize = 16;
        let name_w: usize = 44;
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "environment:",
            "AIFO_API_KEY",
            icon(has_key),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "AIFO_API_BASE",
            icon(has_base),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "AIFO_API_VERSION",
            icon(has_version),
            label_w = label_w,
            name_w = name_w
        );
    }
    eprintln!();

    // Workspace write test to validate mounts and UID mapping
    if aifo_coder::container_runtime_path().is_ok() {
        let image = default_image_for_quiet("crush");
        let tmpname = format!(
            ".aifo-coder-doctor-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        let pwd = match std::env::current_dir() {
            Ok(p) => std::fs::canonicalize(&p).unwrap_or(p),
            Err(_) => PathBuf::from("."),
        };
        let uid = Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "0".to_string());
        let gid = Command::new("id")
            .arg("-g")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "0".to_string());

        // Run a short-lived container to validate workspace mount writeability; silence its output
        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("--rm")
            .arg("--user")
            .arg(format!("{uid}:{gid}"))
            .arg("-v")
            .arg(format!("{}:/workspace", pwd.display()))
            .arg("-w")
            .arg("/workspace")
            .arg("-e")
            .arg("HOME=/home/coder")
            .arg("-e")
            .arg("GNUPGHOME=/home/coder/.gnupg")
            .arg(&image)
            .arg("sh")
            .arg("-lc")
            .arg(format!(
                "echo ok > /workspace/{tmp} && id -u > /workspace/{tmp}.uid",
                tmp = tmpname
            ));
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();

        let host_file = pwd.join(&tmpname);
        let host_uid_file = pwd.join(format!("{tmp}.uid", tmp = tmpname));
        if host_file.exists() && host_uid_file.exists() {
            // Present readiness line aligned with the first status column (found/missing)
            let use_color = atty::is(atty::Stream::Stderr);
            let label_width: usize = 16;
            let path_col: usize = 52;
            let yes_val = if use_color {
                "\x1b[34;1myes\x1b[0m".to_string()
            } else {
                "yes".to_string()
            };
            let status_plain = "✅ workspace ready".to_string();
            let status_colored = if use_color {
                format!("\x1b[32m{}\x1b[0m", status_plain)
            } else {
                status_plain
            };
            eprintln!(
                "  {:label_width$} {:<path_col$} {}",
                "workspace writable:",
                yes_val,
                status_colored,
                label_width = label_width,
                path_col = path_col
            );
            let _ = fs::remove_file(&host_file);
            let _ = fs::remove_file(&host_uid_file);
        } else {
            // Fallback: if docker check failed, try host write test to confirm workspace directory is writable
            let host_write_ok = fs::write(&host_file, b"ok\n").is_ok()
                && fs::write(&host_uid_file, format!("{}\n", uid)).is_ok();
            if host_write_ok {
                // Present readiness line aligned with the first status column (found/missing)
                let use_color = atty::is(atty::Stream::Stderr);
                let label_width: usize = 16;
                let path_col: usize = 52;
                let yes_val = if use_color {
                    "\x1b[34;1myes\x1b[0m".to_string()
                } else {
                    "yes".to_string()
                };
                let status_plain = "✅ workspace ready".to_string();
                let status_colored = if use_color {
                    format!("\x1b[32m{}\x1b[0m", status_plain)
                } else {
                    status_plain
                };
                eprintln!(
                    "  {:label_width$} {:<path_col$} {}",
                    "workspace writable:",
                    yes_val,
                    status_colored,
                    label_width = label_width,
                    path_col = path_col
                );
                let _ = fs::remove_file(&host_file);
                let _ = fs::remove_file(&host_uid_file);
            } else {
                // On failure, report clearly without polluting stderr with container logs
                let use_color = atty::is(atty::Stream::Stderr);
                let label_width: usize = 16;
                let path_col: usize = 44;
                let no_val = if use_color {
                    "\x1b[34;1mno\x1b[0m".to_string()
                } else {
                    "no".to_string()
                };
                let status_plain = "❌ workspace not writable".to_string();
                let status_colored = if use_color {
                    format!("\x1b[31m{}\x1b[0m", status_plain)
                } else {
                    status_plain
                };
                eprintln!(
                    "  {:label_width$} {:<path_col$} {}",
                    "workspace writable:",
                    no_val,
                    status_colored,
                    label_width = label_width,
                    path_col = path_col
                );
            }
        }
    }

    eprintln!();
    eprintln!("doctor: completed diagnostics.");
    eprintln!();
}

#[derive(Parser, Debug)]
#[command(
    name = "aifo-coder",
    version,
    about = "Run Codex, Crush or Aider inside Docker with current directory mounted.",
    override_usage = "aifo-coder [OPTIONS] <COMMAND> [-- [AGENT-OPTIONS]]",
    after_long_help = "Examples:\n  aifo-coder --fork 2 aider -- --help\n  aifo-coder --fork 3 --fork-include-dirty --fork-session-name aifo-work aider --\n  aifo-coder fork list --json\n  aifo-coder fork clean --older-than 14 --yes\n\n",
    after_help = "\n"
)]
struct Cli {
    /// Override Docker image (full ref). If unset, use per-agent default: {prefix}-{agent}:{tag}
    #[arg(long)]
    image: Option<String>,

    /// Attach language toolchains and inject PATH shims (repeatable)
    #[arg(long = "toolchain", value_enum)]
    toolchain: Vec<ToolchainKind>,

    /// Attach toolchains with optional versions (repeatable), e.g. rust@1.80, node@20, python@3.12
    #[arg(long = "toolchain-spec")]
    toolchain_spec: Vec<String>,

    /// Override image(s) for toolchains (repeatable, kind=image)
    #[arg(long = "toolchain-image")]
    toolchain_image: Vec<String>,

    /// Disable named cache volumes for toolchain sidecars
    #[arg(long = "no-toolchain-cache")]
    no_toolchain_cache: bool,

    /// Use Linux unix socket transport for tool-exec proxy (instead of TCP)
    #[arg(long = "toolchain-unix-socket")]
    toolchain_unix_socket: bool,

    /// Bootstrap actions for toolchains (repeatable), e.g. typescript=global
    #[arg(long = "toolchain-bootstrap")]
    toolchain_bootstrap: Vec<String>,

    /// Print detailed execution info
    #[arg(long)]
    verbose: bool,

    /// Choose image flavor: full or slim (overrides AIFO_CODER_IMAGE_FLAVOR)
    #[arg(long, value_enum)]
    flavor: Option<Flavor>,

    /// Invalidate on-disk registry cache before probing
    #[arg(long)]
    invalidate_registry_cache: bool,

    /// Prepare and print what would run, but do not execute
    #[arg(long)]
    dry_run: bool,

    /// Colorize output: auto|always|never
    #[arg(long = "color", value_enum)]
    color: Option<aifo_coder::ColorMode>,

    /// Fork mode: create N panes (N>=2) in tmux/Windows Terminal with cloned workspaces
    #[arg(long)]
    fork: Option<usize>,

    /// Include uncommitted changes via snapshot commit (temporary index + commit-tree; no hooks/signing)
    #[arg(long = "fork-include-dirty")]
    fork_include_dirty: bool,

    /// Clone with --dissociate for independence
    #[arg(long = "fork-dissociate")]
    fork_dissociate: bool,

    /// Session/window name override
    #[arg(long = "fork-session-name")]
    fork_session_name: Option<String>,

    /// Layout for tmux panes: tiled, even-h, or even-v
    #[arg(long = "fork-layout", value_parser = validate_layout)]
    fork_layout: Option<String>,

    /// Keep created clones on orchestration failure (default: keep)
    #[arg(long = "fork-keep-on-failure", default_value_t = true)]
    fork_keep_on_failure: bool,

    /// Post-fork merge strategy to apply after all panes exit (default: octopus)
    #[arg(long = "fork-merge-strategy", value_enum, default_value_t = aifo_coder::MergingStrategy::Octopus, hide_default_value = true)]
    fork_merging_strategy: aifo_coder::MergingStrategy,

    /// Disable automatic disposal of the fork session after a successful octopus merge (default: enabled)
    #[arg(long = "fork-merge-no-autoclean", default_value_t = true, action = clap::ArgAction::SetFalse)]
    fork_merging_autoclean: bool,

    #[command(subcommand)]
    command: Agent,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, clap::ValueEnum)]
enum Flavor {
    Full,
    Slim,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, clap::ValueEnum)]
enum ToolchainKind {
    Rust,
    Node,
    #[value(alias = "ts")]
    Typescript,
    Python,
    #[value(alias = "ccpp")]
    #[value(alias = "c")]
    #[value(alias = "cpp")]
    #[value(alias = "c_cpp")]
    #[value(alias = "c++")]
    CCpp,
    Go,
}

impl ToolchainKind {
    fn as_str(&self) -> &'static str {
        match self {
            ToolchainKind::Rust => "rust",
            ToolchainKind::Node => "node",
            ToolchainKind::Typescript => "typescript",
            ToolchainKind::Python => "python",
            ToolchainKind::CCpp => "c-cpp",
            ToolchainKind::Go => "go",
        }
    }
}

// Validate tmux layout flag value
fn validate_layout(s: &str) -> Result<String, String> {
    match s {
        "tiled" | "even-h" | "even-v" => Ok(s.to_string()),
        _ => Err("must be one of tiled, even-h, even-v".to_string()),
    }
}

// Build child args for panes by reconstructing from parsed Cli, stripping fork flags.
fn fork_build_child_args(cli: &Cli) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();

    if let Some(img) = cli.image.as_deref() {
        if !img.trim().is_empty() {
            args.push("--image".to_string());
            args.push(img.to_string());
        }
    }
    for k in &cli.toolchain {
        args.push("--toolchain".to_string());
        args.push(k.as_str().to_string());
    }
    for s in &cli.toolchain_spec {
        args.push("--toolchain-spec".to_string());
        args.push(s.clone());
    }
    for ti in &cli.toolchain_image {
        args.push("--toolchain-image".to_string());
        args.push(ti.clone());
    }
    if cli.no_toolchain_cache {
        args.push("--no-toolchain-cache".to_string());
    }
    if cli.toolchain_unix_socket {
        args.push("--toolchain-unix-socket".to_string());
    }
    for b in &cli.toolchain_bootstrap {
        args.push("--toolchain-bootstrap".to_string());
        args.push(b.clone());
    }
    if cli.verbose {
        args.push("--verbose".to_string());
    }
    if let Some(fl) = cli.flavor {
        args.push("--flavor".to_string());
        args.push(
            match fl {
                Flavor::Full => "full",
                Flavor::Slim => "slim",
            }
            .to_string(),
        );
    }
    if cli.invalidate_registry_cache {
        args.push("--invalidate-registry-cache".to_string());
    }
    if cli.dry_run {
        args.push("--dry-run".to_string());
    }

    // Subcommand and its args
    match &cli.command {
        Agent::Codex { args: a } => {
            args.push("codex".to_string());
            args.extend(a.clone());
        }
        Agent::Crush { args: a } => {
            args.push("crush".to_string());
            args.extend(a.clone());
        }
        Agent::Aider { args: a } => {
            args.push("aider".to_string());
            args.extend(a.clone());
        }
        // For non-agent subcommands, default to aider to avoid starting doctor/images in panes.
        _ => {
            args.push("aider".to_string());
        }
    }

    args
}

fn print_inspect_merge_guidance(
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

// Orchestrate tmux-based fork session (Linux/macOS/WSL)
fn fork_run(cli: &Cli, panes: usize) -> ExitCode {
    // Preflight
    if which("git").is_err() {
        eprintln!("aifo-coder: error: git is required and was not found in PATH.");
        return ExitCode::from(1);
    }
    if cfg!(target_os = "windows") {
        // Windows preflight: require at least one orchestrator (wt.exe, PowerShell, or Git Bash)
        let wt_ok = which("wt").or_else(|_| which("wt.exe")).is_ok();
        let ps_ok = which("pwsh")
            .or_else(|_| which("powershell"))
            .or_else(|_| which("powershell.exe"))
            .is_ok();
        let gb_ok = which("git-bash.exe")
            .or_else(|_| which("bash.exe"))
            .or_else(|_| which("mintty.exe"))
            .is_ok();
        if !(wt_ok || ps_ok || gb_ok) {
            eprintln!("aifo-coder: error: none of Windows Terminal (wt.exe), PowerShell, or Git Bash were found in PATH.");
            return ExitCode::from(127);
        }
    } else {
        if which("tmux").is_err() {
            eprintln!("aifo-coder: error: tmux not found. Please install tmux to use fork mode.");
            return ExitCode::from(127);
        }
    }
    let repo_root = match aifo_coder::repo_root() {
        Some(p) => p,
        None => {
            eprintln!("aifo-coder: error: fork mode must be run inside a Git repository.");
            return ExitCode::from(1);
        }
    };
    if panes > 8 {
        let msg = format!(
            "Launching {} panes may impact disk/memory and I/O performance.",
            panes
        );
        if !aifo_coder::warn_prompt_continue_or_quit(&[&msg]) {
            return ExitCode::from(1);
        }
    }

    // Identify base
    let (base_label, mut base_ref_or_sha, base_commit_sha) =
        match aifo_coder::fork_base_info(&repo_root) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("aifo-coder: error determining base: {}", e);
                return ExitCode::from(1);
            }
        };

    // Session id and name
    let sid = aifo_coder::create_session_id();
    let session_name = cli
        .fork_session_name
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("aifo-{}", sid));

    // Snapshot when requested
    let mut snapshot_sha: Option<String> = None;
    if cli.fork_include_dirty {
        match aifo_coder::fork_create_snapshot(&repo_root, &sid) {
            Ok(sha) => {
                snapshot_sha = Some(sha.clone());
                base_ref_or_sha = sha;
            }
            Err(e) => {
                let msg = format!("Failed to create snapshot of dirty working tree: {}", e);
                if !aifo_coder::warn_prompt_continue_or_quit(&[
                    &msg,
                    "The fork panes will NOT include your uncommitted changes.",
                ]) {
                    return ExitCode::from(1);
                }
            }
        }
    } else {
        // Warn if dirty but not including
        if let Ok(out) = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .arg("status")
            .arg("--porcelain=v1")
            .arg("-uall")
            .output()
        {
            if !out.stdout.is_empty() {
                if !aifo_coder::warn_prompt_continue_or_quit(&[
                    "Working tree has uncommitted changes; they will NOT be included in the fork panes.",
                    "Re-run with --fork-include-dirty to include them.",
                ]) {
                    return ExitCode::from(1);
                }
            }
        }
    }

    // Preflight: if octopus merging requested, ensure original repo is clean to avoid hidden merge failures
    if matches!(
        cli.fork_merging_strategy,
        aifo_coder::MergingStrategy::Octopus
    ) {
        if let Ok(o) = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .arg("status")
            .arg("--porcelain=v1")
            .arg("-uall")
            .output()
        {
            if !o.stdout.is_empty() {
                if !aifo_coder::warn_prompt_continue_or_quit(&[
                    "Octopus merge requires a clean working tree in the original repository.",
                    "Commit or stash your changes before proceeding, or merging will likely fail.",
                ]) {
                    return ExitCode::from(1);
                }
            }
        }
    }
    // Preflight: warn once about missing toolchains and allow abort
    {
        let agent_for_warn = match &cli.command {
            Agent::Codex { .. } => "codex",
            Agent::Crush { .. } => "crush",
            Agent::Aider { .. } => "aider",
            _ => "aider",
        };
        if !maybe_warn_missing_toolchain_for_fork(cli, agent_for_warn) {
            return ExitCode::from(1);
        }
    }
    // Create clones
    let dissoc = cli.fork_dissociate;
    let clones = match aifo_coder::fork_clone_and_checkout_panes(
        &repo_root,
        &sid,
        panes,
        &base_ref_or_sha,
        &base_label,
        dissoc,
    ) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("aifo-coder: error during cloning: {}", e);
            return ExitCode::from(1);
        }
    };

    // Prepare per-pane env/state dirs
    let agent = match &cli.command {
        Agent::Codex { .. } => "codex",
        Agent::Crush { .. } => "crush",
        Agent::Aider { .. } => "aider",
        _ => "aider",
    };
    let state_base = env::var("AIFO_CODER_FORK_STATE_BASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            home::home_dir()
                .unwrap_or_else(|| PathBuf::from("~"))
                .join(".aifo-coder")
                .join("state")
        });
    let session_dir = aifo_coder::fork_session_dir(&repo_root, &sid);

    // Summary header
    let use_color_out = atty::is(atty::Stream::Stdout);
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
    if let Some(ref snap) = snapshot_sha {
        if use_color_out {
            println!(
                "\x1b[32mincluded dirty working tree via snapshot {}\x1b[0m",
                snap
            );
        } else {
            println!("included dirty working tree via snapshot {}", snap);
        }
    } else if cli.fork_include_dirty {
        if use_color_out {
            println!("\x1b[33mwarning:\x1b[0m requested --fork-include-dirty, but snapshot failed; dirty changes not included.");
        } else {
            println!("warning: requested --fork-include-dirty, but snapshot failed; dirty changes not included.");
        }
    }
    if !dissoc {
        if use_color_out {
            println!("\x1b[90mnote: clones reference the base repo’s object store; avoid pruning base objects until done.\x1b[0m");
        } else {
            println!("note: clones reference the base repo’s object store; avoid pruning base objects until done.");
        }
    }
    println!();

    // Per-pane run
    let child_args = fork_build_child_args(cli);
    let layout = cli.fork_layout.as_deref().unwrap_or("tiled").to_string();
    let layout_effective = match layout.as_str() {
        "even-h" => "even-horizontal".to_string(),
        "even-v" => "even-vertical".to_string(),
        _ => "tiled".to_string(),
    };
    if cli.verbose {
        eprintln!(
            "aifo-coder: tmux layout requested: {} -> effective: {}",
            layout, layout_effective
        );
    }

    // Write metadata skeleton
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_secs();
    // Determine the commit SHA used as the checkout base for metadata:
    // - Use snapshot SHA when include-dirty snapshot was created
    // - Otherwise resolve base_ref_or_sha to a SHA (branch or SHA), fall back to HEAD SHA from fork_base_info
    let base_commit_sha_for_meta = if let Some(ref snap) = snapshot_sha {
        snap.clone()
    } else {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo_root)
            .arg("rev-parse")
            .arg("--verify")
            .arg(&base_ref_or_sha)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output()
            .ok();
        out.and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| base_commit_sha.clone())
    };
    // Shadow base_commit_sha so existing metadata builders pick the correct SHA
    let base_commit_sha = base_commit_sha_for_meta.clone();
    let pane_dirs_vec: Vec<String> = clones
        .iter()
        .map(|(p, _b)| p.display().to_string())
        .collect();
    let branches_vec: Vec<String> = clones.iter().map(|(_p, b)| b.clone()).collect();
    let mut meta = format!(
        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
        created_at,
        aifo_coder::json_escape(&base_label),
        aifo_coder::json_escape(&base_ref_or_sha),
        aifo_coder::json_escape(&base_commit_sha),
        panes,
        pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
        branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
        aifo_coder::json_escape(&layout)
    );
    if let Some(ref snap) = snapshot_sha {
        meta.push_str(&format!(
            ", \"snapshot_sha\": {}",
            aifo_coder::json_escape(snap)
        ));
    }
    meta.push_str(" }");
    let _ = fs::create_dir_all(&session_dir);
    let _ = fs::write(session_dir.join(".meta.json"), meta);

    // Print per-pane info lines
    for (idx, (pane_dir, branch)) in clones.iter().enumerate() {
        let i = idx + 1;
        let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
        let state_dir = state_base.join(&sid).join(format!("pane-{}", i));
        let _ = fs::create_dir_all(state_dir.join(".aider"));
        let _ = fs::create_dir_all(state_dir.join(".codex"));
        let _ = fs::create_dir_all(state_dir.join(".crush"));
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

    // Orchestrate panes (Windows uses Windows Terminal or PowerShell; Unix-like uses tmux)
    if cfg!(target_os = "windows") {
        // Helper to PowerShell-quote a single token
        let ps_quote = |s: &str| -> String {
            let esc = s.replace('\'', "''");
            format!("'{}'", esc)
        };
        // Build inner PowerShell command string setting env per pane, then invoking aifo-coder with args
        let build_ps_inner =
            |i: usize, pane_dir: &std::path::Path, pane_state_dir: &PathBuf| -> String {
                let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
                let kv = [
                    ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
                    ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
                    ("AIFO_CODER_HOSTNAME", cname),
                    ("AIFO_CODER_FORK_SESSION", sid.clone()),
                    ("AIFO_CODER_FORK_INDEX", i.to_string()),
                    (
                        "AIFO_CODER_FORK_STATE_DIR",
                        pane_state_dir.display().to_string(),
                    ),
                    ("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING", "1".to_string()),
                ];
                let mut assigns: Vec<String> = Vec::new();
                for (k, v) in kv {
                    assigns.push(format!("$env:{}={}", k, ps_quote(&v)));
                }
                let mut words: Vec<String> = vec!["aifo-coder".to_string()];
                words.extend(child_args.clone());
                let cmd = words
                    .iter()
                    .map(|w| ps_quote(w))
                    .collect::<Vec<_>>()
                    .join(" ");
                let setloc = format!("Set-Location {}", ps_quote(&pane_dir.display().to_string()));
                format!("{}; {}; {}", setloc, assigns.join("; "), cmd)
            };
        // Build inner Git Bash command string setting env per pane, then invoking aifo-coder with args; keeps shell open
        let build_bash_inner =
            |i: usize, pane_dir: &std::path::Path, pane_state_dir: &PathBuf| -> String {
                let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
                let kv = [
                    ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
                    ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
                    ("AIFO_CODER_HOSTNAME", cname),
                    ("AIFO_CODER_FORK_SESSION", sid.clone()),
                    ("AIFO_CODER_FORK_INDEX", i.to_string()),
                    (
                        "AIFO_CODER_FORK_STATE_DIR",
                        pane_state_dir.display().to_string(),
                    ),
                    ("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING", "1".to_string()),
                ];
                let mut exports: Vec<String> = Vec::new();
                for (k, v) in kv {
                    exports.push(format!("export {}={}", k, aifo_coder::shell_escape(&v)));
                }
                let mut words: Vec<String> = vec!["aifo-coder".to_string()];
                words.extend(child_args.clone());
                let cmd = aifo_coder::shell_join(&words);
                let cddir = aifo_coder::shell_escape(&pane_dir.display().to_string());
                let tail = if matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None)
                {
                    " && exec bash"
                } else {
                    ""
                };
                format!("cd {} && {}; {}{}", cddir, exports.join("; "), cmd, tail)
            };

        // Orchestrator preference override (optional): AIFO_CODER_FORK_ORCH={gitbash|powershell}
        let orch_pref = env::var("AIFO_CODER_FORK_ORCH")
            .ok()
            .map(|s| s.to_ascii_lowercase());
        if orch_pref.as_deref() == Some("gitbash") {
            // Force Git Bash orchestrator if available
            let gitbash = which("git-bash.exe").or_else(|_| which("bash.exe"));
            if let Ok(gb) = gitbash {
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let inner = build_bash_inner(i, pane_dir.as_path(), &pane_state_dir);

                    let mut cmd = Command::new(&gb);
                    cmd.arg("-c").arg(&inner);
                    if cli.verbose {
                        let preview =
                            vec![gb.display().to_string(), "-c".to_string(), inner.clone()];
                        eprintln!("aifo-coder: git-bash: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more Git Bash windows.");
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let panes_created = existing.len();
                    let pane_dirs_vec: Vec<String> = existing
                        .iter()
                        .map(|(p, _)| p.display().to_string())
                        .collect();
                    let branches_vec: Vec<String> =
                        existing.iter().map(|(_, b)| b.clone()).collect();
                    let mut meta2 = format!(
                        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                        created_at,
                        aifo_coder::json_escape(&base_label),
                        aifo_coder::json_escape(&base_ref_or_sha),
                        aifo_coder::json_escape(&base_commit_sha),
                        panes,
                        panes_created,
                        pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        aifo_coder::json_escape(&layout)
                    );
                    if let Some(ref snap) = snapshot_sha {
                        meta2.push_str(&format!(
                            ", \"snapshot_sha\": {}",
                            aifo_coder::json_escape(snap)
                        ));
                    }
                    meta2.push_str(" }");
                    let _ = fs::write(session_dir.join(".meta.json"), meta2);
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[36;1m",
                                &format!(
                                    "aifo-coder: applying post-fork merge strategy: {}",
                                    strat
                                )
                            )
                        );
                    }
                    match aifo_coder::fork_merge_branches_by_session(
                        &repo_root,
                        &sid,
                        cli.fork_merging_strategy,
                        cli.verbose,
                        cli.dry_run,
                    ) {
                        Ok(()) => {
                            {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[32;1m",
                                        &format!(
                                            "aifo-coder: merge strategy '{}' completed.",
                                            strat
                                        )
                                    )
                                );
                            }
                            if matches!(
                                cli.fork_merging_strategy,
                                aifo_coder::MergingStrategy::Octopus
                            ) && cli.fork_merging_autoclean
                                && !cli.dry_run
                            {
                                eprintln!();
                                {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[36;1m",
                                            &format!(
                                                "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                                sid
                                            )
                                        )
                                    );
                                }
                                let opts = aifo_coder::ForkCleanOpts {
                                    session: Some(sid.clone()),
                                    older_than_days: None,
                                    all: false,
                                    dry_run: false,
                                    yes: true,
                                    force: true,
                                    keep_dirty: false,
                                    json: false,
                                };
                                match aifo_coder::fork_clean(&repo_root, &opts) {
                                    Ok(_) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[33m",
                                                &format!(
                                                    "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                    sid, e
                                                )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[31;1m",
                                    &format!(
                                        "aifo-coder: merge strategy '{}' failed: {}",
                                        strat, e
                                    )
                                )
                            );
                        }
                    }
                }
                println!();
                println!("aifo-coder: fork session {} launched (Git Bash).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    true,
                    false,
                );
                return ExitCode::from(0);
            } else if let Ok(mt) = which("mintty.exe") {
                // Use mintty as a Git Bash UI launcher
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let inner = build_bash_inner(i, pane_dir.as_path(), &pane_state_dir);

                    let mut cmd = Command::new(&mt);
                    cmd.arg("-e").arg("bash").arg("-lc").arg(&inner);
                    if cli.verbose {
                        let preview = vec![
                            mt.display().to_string(),
                            "-e".to_string(),
                            "bash".to_string(),
                            "-lc".to_string(),
                            inner.clone(),
                        ];
                        eprintln!("aifo-coder: mintty: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more mintty windows.");
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let panes_created = existing.len();
                    let pane_dirs_vec: Vec<String> = existing
                        .iter()
                        .map(|(p, _)| p.display().to_string())
                        .collect();
                    let branches_vec: Vec<String> =
                        existing.iter().map(|(_, b)| b.clone()).collect();
                    let mut meta2 = format!(
                        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                        created_at,
                        aifo_coder::json_escape(&base_label),
                        aifo_coder::json_escape(&base_ref_or_sha),
                        aifo_coder::json_escape(&base_commit_sha),
                        panes,
                        panes_created,
                        pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        aifo_coder::json_escape(&layout)
                    );
                    if let Some(ref snap) = snapshot_sha {
                        meta2.push_str(&format!(
                            ", \"snapshot_sha\": {}",
                            aifo_coder::json_escape(snap)
                        ));
                    }
                    meta2.push_str(" }");
                    let _ = fs::write(session_dir.join(".meta.json"), meta2);
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[36;1m",
                                &format!(
                                    "aifo-coder: applying post-fork merge strategy: {}",
                                    strat
                                )
                            )
                        );
                    }
                    match aifo_coder::fork_merge_branches_by_session(
                        &repo_root,
                        &sid,
                        cli.fork_merging_strategy,
                        cli.verbose,
                        cli.dry_run,
                    ) {
                        Ok(()) => {
                            {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[32;1m",
                                        &format!(
                                            "aifo-coder: merge strategy '{}' completed.",
                                            strat
                                        )
                                    )
                                );
                            }
                            if matches!(
                                cli.fork_merging_strategy,
                                aifo_coder::MergingStrategy::Octopus
                            ) && cli.fork_merging_autoclean
                                && !cli.dry_run
                            {
                                eprintln!();
                                {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[36;1m",
                                            &format!(
                                                "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                                sid
                                            )
                                        )
                                    );
                                }
                                let opts = aifo_coder::ForkCleanOpts {
                                    session: Some(sid.clone()),
                                    older_than_days: None,
                                    all: false,
                                    dry_run: false,
                                    yes: true,
                                    force: true,
                                    keep_dirty: false,
                                    json: false,
                                };
                                match aifo_coder::fork_clean(&repo_root, &opts) {
                                    Ok(_) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[33m",
                                                &format!(
                                                    "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                    sid, e
                                                )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[31;1m",
                                    &format!(
                                        "aifo-coder: merge strategy '{}' failed: {}",
                                        strat, e
                                    )
                                )
                            );
                        }
                    }
                }
                println!();
                println!("aifo-coder: fork session {} launched (mintty).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    true,
                    false,
                );
                return ExitCode::from(0);
            } else {
                eprintln!("aifo-coder: error: AIFO_CODER_FORK_ORCH=gitbash requested but Git Bash/mintty were not found in PATH.");
                return ExitCode::from(1);
            }
        } else if orch_pref.as_deref() == Some("powershell") {
            // Fall through to PowerShell windows launcher below, bypassing Windows Terminal
        }
        // Prefer Windows Terminal (wt.exe)
        let wt = which("wt").or_else(|_| which("wt.exe"));
        if let Ok(wtbin) = wt {
            if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                {
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[33m",
                            "aifo-coder: using PowerShell windows to enable post-fork merging (--fork-merge-strategy)."
                        )
                    );
                }
            } else {
                if clones.is_empty() {
                    eprintln!("aifo-coder: no panes to create.");
                    return ExitCode::from(1);
                }
                let psbin = which("pwsh")
                    .or_else(|_| which("powershell"))
                    .or_else(|_| which("powershell.exe"))
                    .unwrap_or_else(|_| std::path::PathBuf::from("powershell"));
                let orient_for_layout = |i: usize| -> &'static str {
                    match layout.as_str() {
                        "even-h" => "-H",
                        "even-v" => "-V",
                        _ => {
                            // tiled: alternate for some balance
                            if i % 2 == 0 {
                                "-H"
                            } else {
                                "-V"
                            }
                        }
                    }
                };

                // Pane 1: new tab
                {
                    let (pane1_dir, _b) = &clones[0];
                    let pane_state_dir = state_base.join(&sid).join("pane-1");
                    let inner = build_ps_inner(1, pane1_dir.as_path(), &pane_state_dir);
                    let mut cmd = Command::new(&wtbin);
                    cmd.arg("new-tab")
                        .arg("-d")
                        .arg(pane1_dir)
                        .arg(&psbin)
                        .arg("-NoExit")
                        .arg("-Command")
                        .arg(&inner);
                    if cli.verbose {
                        let preview = vec![
                            "wt".to_string(),
                            "new-tab".to_string(),
                            "-d".to_string(),
                            pane1_dir.display().to_string(),
                            psbin.display().to_string(),
                            "-NoExit".to_string(),
                            "-Command".to_string(),
                            inner.clone(),
                        ];
                        eprintln!(
                            "aifo-coder: windows-terminal: {}",
                            aifo_coder::shell_join(&preview)
                        );
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        Ok(_) => {
                            eprintln!("aifo-coder: Windows Terminal failed to start first pane (non-zero exit).");
                            if !cli.fork_keep_on_failure {
                                for (dir, _) in &clones {
                                    let _ = fs::remove_dir_all(dir);
                                }
                                println!(
                                    "Removed all created pane directories under {}.",
                                    session_dir.display()
                                );
                            } else {
                                println!(
                                    "Clones remain under {} for recovery.",
                                    session_dir.display()
                                );
                            }
                            // Update metadata with panes_created
                            let existing: Vec<(PathBuf, String)> = clones
                                .iter()
                                .filter(|(p, _)| p.exists())
                                .map(|(p, b)| (p.clone(), b.clone()))
                                .collect();
                            let panes_created = existing.len();
                            let pane_dirs_vec: Vec<String> = existing
                                .iter()
                                .map(|(p, _)| p.display().to_string())
                                .collect();
                            let branches_vec: Vec<String> =
                                existing.iter().map(|(_, b)| b.clone()).collect();
                            let mut meta2 = format!(
                            "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                            created_at,
                            aifo_coder::json_escape(&base_label),
                            aifo_coder::json_escape(&base_ref_or_sha),
                            aifo_coder::json_escape(&base_commit_sha),
                            panes,
                            panes_created,
                            pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                            branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                            aifo_coder::json_escape(&layout)
                        );
                            if let Some(ref snap) = snapshot_sha {
                                meta2.push_str(&format!(
                                    ", \"snapshot_sha\": {}",
                                    aifo_coder::json_escape(snap)
                                ));
                            }
                            meta2.push_str(" }");
                            let _ = fs::write(session_dir.join(".meta.json"), meta2);
                            return ExitCode::from(1);
                        }
                        Err(e) => {
                            eprintln!(
                                "aifo-coder: Windows Terminal failed to start first pane: {}",
                                e
                            );
                            if !cli.fork_keep_on_failure {
                                for (dir, _) in &clones {
                                    let _ = fs::remove_dir_all(dir);
                                }
                                println!(
                                    "Removed all created pane directories under {}.",
                                    session_dir.display()
                                );
                            } else {
                                println!(
                                    "Clones remain under {} for recovery.",
                                    session_dir.display()
                                );
                            }
                            // Update metadata with panes_created
                            let existing: Vec<(PathBuf, String)> = clones
                                .iter()
                                .filter(|(p, _)| p.exists())
                                .map(|(p, b)| (p.clone(), b.clone()))
                                .collect();
                            let panes_created = existing.len();
                            let pane_dirs_vec: Vec<String> = existing
                                .iter()
                                .map(|(p, _)| p.display().to_string())
                                .collect();
                            let branches_vec: Vec<String> =
                                existing.iter().map(|(_, b)| b.clone()).collect();
                            let mut meta2 = format!(
                            "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                            created_at,
                            aifo_coder::json_escape(&base_label),
                            aifo_coder::json_escape(&base_ref_or_sha),
                            aifo_coder::json_escape(&base_commit_sha),
                            panes,
                            panes_created,
                            pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                            branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                            aifo_coder::json_escape(&layout)
                        );
                            if let Some(ref snap) = snapshot_sha {
                                meta2.push_str(&format!(
                                    ", \"snapshot_sha\": {}",
                                    aifo_coder::json_escape(snap)
                                ));
                            }
                            meta2.push_str(" }");
                            let _ = fs::write(session_dir.join(".meta.json"), meta2);
                            return ExitCode::from(1);
                        }
                    }
                }

                // Additional panes: split-pane
                let mut split_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let inner = build_ps_inner(i, pane_dir.as_path(), &pane_state_dir);
                    let orient = orient_for_layout(i);
                    let mut cmd = Command::new(&wtbin);
                    cmd.arg("split-pane")
                        .arg(orient)
                        .arg("-d")
                        .arg(pane_dir)
                        .arg(&psbin)
                        .arg("-NoExit")
                        .arg("-Command")
                        .arg(&inner);
                    if cli.verbose {
                        let preview = vec![
                            "wt".to_string(),
                            "split-pane".to_string(),
                            orient.to_string(),
                            "-d".to_string(),
                            pane_dir.display().to_string(),
                            psbin.display().to_string(),
                            "-NoExit".to_string(),
                            "-Command".to_string(),
                            inner.clone(),
                        ];
                        eprintln!(
                            "aifo-coder: windows-terminal: {}",
                            aifo_coder::shell_join(&preview)
                        );
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            split_failed = true;
                            break;
                        }
                    }
                }
                if split_failed {
                    eprintln!(
                        "aifo-coder: Windows Terminal split-pane failed for one or more panes."
                    );
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                        if let Some((first_dir, first_branch)) = clones.first() {
                            println!("Example recovery:");
                            println!("  git -C \"{}\" status", first_dir.display());
                            println!(
                                "  git -C \"{}\" log --oneline --decorate -n 20",
                                first_dir.display()
                            );
                            println!(
                                "  git -C \"{}\" remote add fork-{}-1 \"{}\"",
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
                        }
                    }
                    // Update metadata
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let panes_created = existing.len();
                    let pane_dirs_vec: Vec<String> = existing
                        .iter()
                        .map(|(p, _)| p.display().to_string())
                        .collect();
                    let branches_vec: Vec<String> =
                        existing.iter().map(|(_, b)| b.clone()).collect();
                    let mut meta2 = format!(
                    "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                    created_at,
                    aifo_coder::json_escape(&base_label),
                    aifo_coder::json_escape(&base_ref_or_sha),
                    aifo_coder::json_escape(&base_commit_sha),
                    panes,
                    panes_created,
                    pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                    branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                    aifo_coder::json_escape(&layout)
                );
                    if let Some(ref snap) = snapshot_sha {
                        meta2.push_str(&format!(
                            ", \"snapshot_sha\": {}",
                            aifo_coder::json_escape(snap)
                        ));
                    }
                    meta2.push_str(" }");
                    let _ = fs::write(session_dir.join(".meta.json"), meta2);
                    return ExitCode::from(1);
                }

                // Print guidance and return (wt.exe is detached)
                println!();
                println!(
                    "aifo-coder: fork session {} launched in Windows Terminal.",
                    sid
                );
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    false,
                    true,
                );
                return ExitCode::from(0);
            }
        }

        // Fallback: separate PowerShell windows via cmd.exe start
        let powershell = which("pwsh")
            .or_else(|_| which("powershell"))
            .or_else(|_| which("powershell.exe"));
        if powershell.is_err() {
            // Fallback: Git Bash (Git Shell / mintty)
            let gitbash = which("git-bash.exe").or_else(|_| which("bash.exe"));
            if let Ok(gb) = gitbash {
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let inner = build_bash_inner(i, pane_dir.as_path(), &pane_state_dir);

                    let mut cmd = Command::new(&gb);
                    cmd.arg("-c").arg(&inner);
                    if cli.verbose {
                        let preview =
                            vec![gb.display().to_string(), "-c".to_string(), inner.clone()];
                        eprintln!("aifo-coder: git-bash: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more Git Bash windows.");
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let panes_created = existing.len();
                    let pane_dirs_vec: Vec<String> = existing
                        .iter()
                        .map(|(p, _)| p.display().to_string())
                        .collect();
                    let branches_vec: Vec<String> =
                        existing.iter().map(|(_, b)| b.clone()).collect();
                    let mut meta2 = format!(
                        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                        created_at,
                        aifo_coder::json_escape(&base_label),
                        aifo_coder::json_escape(&base_ref_or_sha),
                        aifo_coder::json_escape(&base_commit_sha),
                        panes,
                        panes_created,
                        pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        aifo_coder::json_escape(&layout)
                    );
                    if let Some(ref snap) = snapshot_sha {
                        meta2.push_str(&format!(
                            ", \"snapshot_sha\": {}",
                            aifo_coder::json_escape(snap)
                        ));
                    }
                    meta2.push_str(" }");
                    let _ = fs::write(session_dir.join(".meta.json"), meta2);
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[36;1m",
                                &format!(
                                    "aifo-coder: applying post-fork merge strategy: {}",
                                    strat
                                )
                            )
                        );
                    }
                    match aifo_coder::fork_merge_branches_by_session(
                        &repo_root,
                        &sid,
                        cli.fork_merging_strategy,
                        cli.verbose,
                        cli.dry_run,
                    ) {
                        Ok(()) => {
                            {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[32;1m",
                                        &format!(
                                            "aifo-coder: merge strategy '{}' completed.",
                                            strat
                                        )
                                    )
                                );
                            }
                            if matches!(
                                cli.fork_merging_strategy,
                                aifo_coder::MergingStrategy::Octopus
                            ) && cli.fork_merging_autoclean
                                && !cli.dry_run
                            {
                                eprintln!();
                                eprintln!(
                                    "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                    sid
                                );
                                let opts = aifo_coder::ForkCleanOpts {
                                    session: Some(sid.clone()),
                                    older_than_days: None,
                                    all: false,
                                    dry_run: false,
                                    yes: true,
                                    force: true,
                                    keep_dirty: false,
                                    json: false,
                                };
                                match aifo_coder::fork_clean(&repo_root, &opts) {
                                    Ok(_) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[33m",
                                                &format!(
                                                    "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                    sid, e
                                                )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[31;1m",
                                    &format!(
                                        "aifo-coder: merge strategy '{}' failed: {}",
                                        strat, e
                                    )
                                )
                            );
                        }
                    }
                }
                println!();
                println!("aifo-coder: fork session {} launched (Git Bash).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    false,
                    true,
                );
                return ExitCode::from(0);
            } else if let Ok(mt) = which("mintty.exe") {
                // Use mintty as a Git Bash UI launcher
                let mut any_failed = false;
                for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
                    let i = idx + 1;
                    let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                    let inner = build_bash_inner(i, pane_dir.as_path(), &pane_state_dir);

                    let mut cmd = Command::new(&mt);
                    cmd.arg("-e").arg("bash").arg("-lc").arg(&inner);
                    if cli.verbose {
                        let preview = vec![
                            mt.display().to_string(),
                            "-e".to_string(),
                            "bash".to_string(),
                            "-lc".to_string(),
                            inner.clone(),
                        ];
                        eprintln!("aifo-coder: mintty: {}", aifo_coder::shell_join(&preview));
                    }
                    match cmd.status() {
                        Ok(s) if s.success() => {}
                        _ => {
                            any_failed = true;
                            break;
                        }
                    }
                }

                if any_failed {
                    eprintln!("aifo-coder: failed to launch one or more mintty windows.");
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "Clones remain under {} for recovery.",
                            session_dir.display()
                        );
                    }
                    // Update metadata with panes_created
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let panes_created = existing.len();
                    let pane_dirs_vec: Vec<String> = existing
                        .iter()
                        .map(|(p, _)| p.display().to_string())
                        .collect();
                    let branches_vec: Vec<String> =
                        existing.iter().map(|(_, b)| b.clone()).collect();
                    let mut meta2 = format!(
                        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                        created_at,
                        aifo_coder::json_escape(&base_label),
                        aifo_coder::json_escape(&base_ref_or_sha),
                        aifo_coder::json_escape(&base_commit_sha),
                        panes,
                        panes_created,
                        pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        aifo_coder::json_escape(&layout)
                    );
                    if let Some(ref snap) = snapshot_sha {
                        meta2.push_str(&format!(
                            ", \"snapshot_sha\": {}",
                            aifo_coder::json_escape(snap)
                        ));
                    }
                    meta2.push_str(" }");
                    let _ = fs::write(session_dir.join(".meta.json"), meta2);
                    return ExitCode::from(1);
                }

                // Apply post-fork merging if requested, then print guidance
                if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                    let strat = match cli.fork_merging_strategy {
                        aifo_coder::MergingStrategy::None => "none",
                        aifo_coder::MergingStrategy::Fetch => "fetch",
                        aifo_coder::MergingStrategy::Octopus => "octopus",
                    };
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[36;1m",
                                &format!(
                                    "aifo-coder: applying post-fork merge strategy: {}",
                                    strat
                                )
                            )
                        );
                    }
                    match aifo_coder::fork_merge_branches_by_session(
                        &repo_root,
                        &sid,
                        cli.fork_merging_strategy,
                        cli.verbose,
                        cli.dry_run,
                    ) {
                        Ok(()) => {
                            eprintln!("aifo-coder: merge strategy '{}' completed.", strat);
                            if matches!(
                                cli.fork_merging_strategy,
                                aifo_coder::MergingStrategy::Octopus
                            ) && cli.fork_merging_autoclean
                                && !cli.dry_run
                            {
                                eprintln!(
                                    "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                    sid
                                );
                                let opts = aifo_coder::ForkCleanOpts {
                                    session: Some(sid.clone()),
                                    older_than_days: None,
                                    all: false,
                                    dry_run: false,
                                    yes: true,
                                    force: true,
                                    keep_dirty: false,
                                    json: false,
                                };
                                match aifo_coder::fork_clean(&repo_root, &opts) {
                                    Ok(_) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[32;1m",
                                                &format!(
                                                    "aifo-coder: disposed fork session {}.",
                                                    sid
                                                )
                                            )
                                        );
                                    }
                                    Err(e) => {
                                        let use_err = aifo_coder::color_enabled_stderr();
                                        eprintln!(
                                            "{}",
                                            aifo_coder::paint(
                                                use_err,
                                                "\x1b[33m",
                                                &format!(
                                                    "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                    sid, e
                                                )
                                            )
                                        );
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("aifo-coder: merge strategy '{}' failed: {}", strat, e);
                        }
                    }
                }
                println!();
                println!("aifo-coder: fork session {} launched (mintty).", sid);
                print_inspect_merge_guidance(
                    &repo_root,
                    &sid,
                    &base_label,
                    &base_ref_or_sha,
                    &clones,
                    false,
                    false,
                    true,
                );
                return ExitCode::from(0);
            } else {
                // Fallback: launch Windows Terminal even though we cannot wait; print manual-merge advice
                let wt2 = which("wt").or_else(|_| which("wt.exe"));
                if let Ok(wtbin2) = wt2 {
                    if clones.is_empty() {
                        eprintln!("aifo-coder: no panes to create.");
                        return ExitCode::from(1);
                    }
                    let psbin = which("pwsh")
                        .or_else(|_| which("powershell"))
                        .or_else(|_| which("powershell.exe"))
                        .unwrap_or_else(|_| std::path::PathBuf::from("powershell"));
                    let orient_for_layout = |i: usize| -> &'static str {
                        match layout.as_str() {
                            "even-h" => "-H",
                            "even-v" => "-V",
                            _ => {
                                if i % 2 == 0 {
                                    "-H"
                                } else {
                                    "-V"
                                }
                            }
                        }
                    };

                    // Pane 1
                    {
                        let (pane1_dir, _b) = &clones[0];
                        let pane_state_dir = state_base.join(&sid).join("pane-1");
                        let inner = build_ps_inner(1, pane1_dir.as_path(), &pane_state_dir);
                        let mut cmd = Command::new(&wtbin2);
                        cmd.arg("new-tab")
                            .arg("-d")
                            .arg(pane1_dir)
                            .arg(&psbin)
                            .arg("-NoExit")
                            .arg("-Command")
                            .arg(&inner);
                        if cli.verbose {
                            let preview = vec![
                                "wt".to_string(),
                                "new-tab".to_string(),
                                "-d".to_string(),
                                pane1_dir.display().to_string(),
                                psbin.display().to_string(),
                                "-NoExit".to_string(),
                                "-Command".to_string(),
                                inner.clone(),
                            ];
                            eprintln!(
                                "aifo-coder: windows-terminal: {}",
                                aifo_coder::shell_join(&preview)
                            );
                        }
                        let _ = cmd.status();
                    }

                    // Additional panes
                    let mut split_failed = false;
                    for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
                        let i = idx + 1;
                        let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
                        let inner = build_ps_inner(i, pane_dir.as_path(), &pane_state_dir);
                        let orient = orient_for_layout(i);
                        let mut cmd = Command::new(&wtbin2);
                        cmd.arg("split-pane")
                            .arg(orient)
                            .arg("-d")
                            .arg(pane_dir)
                            .arg(&psbin)
                            .arg("-NoExit")
                            .arg("-Command")
                            .arg(&inner);
                        if cli.verbose {
                            let preview = vec![
                                "wt".to_string(),
                                "split-pane".to_string(),
                                orient.to_string(),
                                "-d".to_string(),
                                pane_dir.display().to_string(),
                                psbin.display().to_string(),
                                "-NoExit".to_string(),
                                "-Command".to_string(),
                                inner.clone(),
                            ];
                            eprintln!(
                                "aifo-coder: windows-terminal: {}",
                                aifo_coder::shell_join(&preview)
                            );
                        }
                        match cmd.status() {
                            Ok(s) if s.success() => {}
                            _ => {
                                split_failed = true;
                                break;
                            }
                        }
                    }
                    if split_failed {
                        eprintln!("aifo-coder: warning: one or more Windows Terminal panes failed to open.");
                    }

                    println!();
                    println!(
                        "aifo-coder: fork session {} launched in Windows Terminal.",
                        sid
                    );
                    if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                        let strat = match cli.fork_merging_strategy {
                            aifo_coder::MergingStrategy::Fetch => "fetch",
                            aifo_coder::MergingStrategy::Octopus => "octopus",
                            _ => "none",
                        };
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[33m",
                                    &format!("aifo-coder: note: no waitable orchestrator found; automatic post-fork merging ({}) is unavailable.", strat)
                                )
                            );
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[33m",
                                    &format!(
                                        "aifo-coder: after you close all panes, run: aifo-coder fork merge --session {} --strategy {}",
                                        sid, strat
                                    )
                                )
                            );
                        }
                    }
                    print_inspect_merge_guidance(
                        &repo_root,
                        &sid,
                        &base_label,
                        &base_ref_or_sha,
                        &clones,
                        false,
                        false,
                        true,
                    );
                    return ExitCode::from(0);
                } else {
                    eprintln!("aifo-coder: error: neither Windows Terminal (wt.exe), PowerShell, nor Git Bash/mintty found in PATH.");
                    return ExitCode::from(1);
                }
            }
        }
        let ps_name = powershell.unwrap(); // used only for reference in logs

        let mut any_failed = false;
        let mut pids: Vec<String> = Vec::new();
        for (idx, (pane_dir, _b)) in clones.iter().enumerate() {
            let i = idx + 1;
            let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
            let inner = build_ps_inner(i, pane_dir.as_path(), &pane_state_dir);

            // Launch a new PowerShell window using Start-Process and capture its PID
            let script = {
                let wd = ps_quote(&pane_dir.display().to_string());
                let child = ps_quote(&ps_name.display().to_string());
                let inner_q = ps_quote(&inner);
                let arglist =
                    if matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                        "'-NoExit','-Command'".to_string()
                    } else {
                        "'-Command'".to_string()
                    };
                format!("(Start-Process -WindowStyle Normal -WorkingDirectory {wd} {child} -ArgumentList {arglist},{inner_q} -PassThru).Id")
            };
            if cli.verbose {
                eprintln!("aifo-coder: powershell start-script: {}", script);
                eprintln!("aifo-coder: powershell detected at: {}", ps_name.display());
            }
            let out = Command::new(&ps_name)
                .arg("-NoProfile")
                .arg("-Command")
                .arg(&script)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    let pid = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if !pid.is_empty() {
                        println!("[{}] started PID={} dir={}", i, pid, pane_dir.display());
                        pids.push(pid.clone());
                    } else {
                        println!("[{}] started dir={} (PID unknown)", i, pane_dir.display());
                    }
                }
                _ => {
                    any_failed = true;
                    break;
                }
            }
        }

        if any_failed {
            eprintln!("aifo-coder: failed to launch one or more PowerShell windows.");
            if !cli.fork_keep_on_failure {
                for (dir, _) in &clones {
                    let _ = fs::remove_dir_all(dir);
                }
                println!(
                    "Removed all created pane directories under {}.",
                    session_dir.display()
                );
            } else {
                println!(
                    "Clones remain under {} for recovery.",
                    session_dir.display()
                );
            }
            // Update metadata with panes_created
            let existing: Vec<(PathBuf, String)> = clones
                .iter()
                .filter(|(p, _)| p.exists())
                .map(|(p, b)| (p.clone(), b.clone()))
                .collect();
            let panes_created = existing.len();
            let pane_dirs_vec: Vec<String> = existing
                .iter()
                .map(|(p, _)| p.display().to_string())
                .collect();
            let branches_vec: Vec<String> = existing.iter().map(|(_, b)| b.clone()).collect();
            let mut meta2 = format!(
                "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                created_at,
                aifo_coder::json_escape(&base_label),
                aifo_coder::json_escape(&base_ref_or_sha),
                aifo_coder::json_escape(&base_commit_sha),
                panes,
                panes_created,
                pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                aifo_coder::json_escape(&layout)
            );
            if let Some(ref snap) = snapshot_sha {
                meta2.push_str(&format!(
                    ", \"snapshot_sha\": {}",
                    aifo_coder::json_escape(snap)
                ));
            }
            meta2.push_str(" }");
            let _ = fs::write(session_dir.join(".meta.json"), meta2);
            return ExitCode::from(1);
        }

        if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
            if !pids.is_empty() {
                let list = pids.join(",");
                let wait_cmd = format!("Wait-Process -Id {}", list);
                if cli.verbose {
                    eprintln!("aifo-coder: powershell wait-script: {}", wait_cmd);
                }
                let _ = Command::new(&ps_name)
                    .arg("-NoProfile")
                    .arg("-Command")
                    .arg(&wait_cmd)
                    .status();
            }
            let strat = match cli.fork_merging_strategy {
                aifo_coder::MergingStrategy::None => "none",
                aifo_coder::MergingStrategy::Fetch => "fetch",
                aifo_coder::MergingStrategy::Octopus => "octopus",
            };
            {
                let use_err = aifo_coder::color_enabled_stderr();
                eprintln!(
                    "{}",
                    aifo_coder::paint(
                        use_err,
                        "\x1b[36;1m",
                        &format!("aifo-coder: applying post-fork merge strategy: {}", strat)
                    )
                );
            }
            match aifo_coder::fork_merge_branches_by_session(
                &repo_root,
                &sid,
                cli.fork_merging_strategy,
                cli.verbose,
                cli.dry_run,
            ) {
                Ok(()) => {
                    {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[32;1m",
                                &format!("aifo-coder: merge strategy '{}' completed.", strat)
                            )
                        );
                    }
                    if matches!(
                        cli.fork_merging_strategy,
                        aifo_coder::MergingStrategy::Octopus
                    ) && cli.fork_merging_autoclean
                        && !cli.dry_run
                    {
                        eprintln!();
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[36;1m",
                                    &format!(
                                        "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                        sid
                                    )
                                )
                            );
                        }
                        let opts = aifo_coder::ForkCleanOpts {
                            session: Some(sid.clone()),
                            older_than_days: None,
                            all: false,
                            dry_run: false,
                            yes: true,
                            force: true,
                            keep_dirty: false,
                            json: false,
                        };
                        match aifo_coder::fork_clean(&repo_root, &opts) {
                            Ok(_) => {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[32;1m",
                                        &format!("aifo-coder: disposed fork session {}.", sid)
                                    )
                                );
                            }
                            Err(e) => {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[33m",
                                        &format!(
                                            "aifo-coder: warning: failed to dispose fork session {}: {}",
                                            sid, e
                                        )
                                    )
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[31;1m",
                            &format!("aifo-coder: merge strategy '{}' failed: {}", strat, e)
                        )
                    );
                }
            }
        }

        // Print guidance and return
        println!();
        println!(
            "aifo-coder: fork session {} launched (PowerShell windows).",
            sid
        );
        print_inspect_merge_guidance(
            &repo_root,
            &sid,
            &base_label,
            &base_ref_or_sha,
            &clones,
            false,
            false,
            true,
        );
        return ExitCode::from(0);
    } else {
        // Build and run tmux session
        let tmux = which("tmux").expect("tmux not found");
        if clones.is_empty() {
            eprintln!("aifo-coder: no panes to create.");
            return ExitCode::from(1);
        }

        // Helper to build inner command string with env exports
        let build_inner = |i: usize, pane_state_dir: &PathBuf| -> String {
            let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
            let mut exports: Vec<String> = Vec::new();
            let kv = [
                ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
                ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
                ("AIFO_CODER_HOSTNAME", cname),
                ("AIFO_CODER_FORK_SESSION", sid.clone()),
                ("AIFO_CODER_FORK_INDEX", i.to_string()),
                (
                    "AIFO_CODER_FORK_STATE_DIR",
                    pane_state_dir.display().to_string(),
                ),
                ("AIFO_CODER_SUPPRESS_TOOLCHAIN_WARNING", "1".to_string()),
            ];
            for (k, v) in kv {
                exports.push(format!("export {}={}", k, aifo_coder::shell_escape(&v)));
            }
            let launcher = std::env::current_exe()
                .ok()
                .and_then(|p| p.canonicalize().ok())
                .and_then(|p| p.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "./aifo-coder".to_string());
            let mut child_cmd_words = vec![launcher];
            child_cmd_words.extend(child_args.clone());
            let child_joined = aifo_coder::shell_join(&child_cmd_words);
            format!(
                r#"#!/usr/bin/env bash
set -e
{}
set +e
{}
st=$?
if [ -t 0 ] && command -v tmux >/dev/null 2>&1; then
  pid="$(tmux display -p '#{{pane_id}}')"
  secs="${{AIFO_CODER_FORK_SHELL_PROMPT_SECS:-5}}"
  printf "aifo-coder: agent exited (code %s). Press 's' to open a shell, or wait: " "$st"
  for ((i=secs; i>=1; i--)); do
    printf "%s " "$i"
    if IFS= read -rsn1 -t 1 ch; then
      echo
      if [[ -z "$ch" || "$ch" == $'\n' || "$ch" == $'\r' ]]; then
        tmux kill-pane -t "$pid" >/dev/null 2>&1 || exit "$st"
        exit "$st"
      elif [[ "$ch" == 's' || "$ch" == 'S' ]]; then
        exec "${{SHELL:-sh}}"
      fi
    fi
  done
  echo
  tmux kill-pane -t "$pid" >/dev/null 2>&1 || exit "$st"
else
  exit "$st"
fi
"#,
                exports.join("\n"),
                child_joined
            )
        };

        // Pane 1
        {
            let (pane1_dir, _b) = &clones[0];
            let _pane_state_dir = state_base.join(&sid).join("pane-1");
            let _inner = build_inner(1, &_pane_state_dir);
            let mut cmd = Command::new(&tmux);
            cmd.arg("new-session")
                .arg("-d")
                .arg("-s")
                .arg(&session_name)
                .arg("-n")
                .arg("aifo-fork")
                .arg("-c")
                .arg(pane1_dir);
            if cli.verbose {
                let preview_new = vec![
                    "tmux".to_string(),
                    "new-session".to_string(),
                    "-d".to_string(),
                    "-s".to_string(),
                    session_name.clone(),
                    "-n".to_string(),
                    "aifo-fork".to_string(),
                    "-c".to_string(),
                    pane1_dir.display().to_string(),
                ];
                eprintln!("aifo-coder: tmux: {}", aifo_coder::shell_join(&preview_new));
            }
            let st = match cmd.status() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("aifo-coder: tmux new-session failed to start: {}", e);
                    // Failure policy: keep clones by default; optionally remove if user disabled keep-on-failure
                    if !cli.fork_keep_on_failure {
                        for (dir, _) in &clones {
                            let _ = fs::remove_dir_all(dir);
                        }
                        println!(
                            "Removed all created pane directories under {}.",
                            session_dir.display()
                        );
                    } else {
                        println!(
                            "One or more clones were created under {}.",
                            session_dir.display()
                        );
                        println!("You can inspect them manually. Example:");
                        if let Some((first_dir, first_branch)) = clones.first() {
                            println!("  git -C \"{}\" status", first_dir.display());
                            println!(
                                "  git -C \"{}\" log --oneline --decorate -n 20",
                                first_dir.display()
                            );
                            println!(
                                "  git -C \"{}\" remote add fork-{}-1 \"{}\"",
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
                        }
                    }
                    // Update metadata with panes_created and existing pane dirs
                    let existing: Vec<(PathBuf, String)> = clones
                        .iter()
                        .filter(|(p, _)| p.exists())
                        .map(|(p, b)| (p.clone(), b.clone()))
                        .collect();
                    let panes_created = existing.len();
                    let pane_dirs_vec: Vec<String> = existing
                        .iter()
                        .map(|(p, _)| p.display().to_string())
                        .collect();
                    let branches_vec: Vec<String> =
                        existing.iter().map(|(_, b)| b.clone()).collect();
                    let mut meta2 = format!(
                        "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                        created_at,
                        aifo_coder::json_escape(&base_label),
                        aifo_coder::json_escape(&base_ref_or_sha),
                        aifo_coder::json_escape(&base_commit_sha),
                        panes,
                        panes_created,
                        pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                        aifo_coder::json_escape(&layout)
                    );
                    if let Some(ref snap) = snapshot_sha {
                        meta2.push_str(&format!(
                            ", \"snapshot_sha\": {}",
                            aifo_coder::json_escape(snap)
                        ));
                    }
                    meta2.push_str(" }");
                    let _ = fs::write(session_dir.join(".meta.json"), meta2);
                    return ExitCode::from(1);
                }
            };
            if !st.success() {
                eprintln!("aifo-coder: tmux new-session failed.");
                // Best-effort: kill any stray session
                let mut kill = Command::new(&tmux);
                let _ = kill
                    .arg("kill-session")
                    .arg("-t")
                    .arg(&session_name)
                    .status();
                if !cli.fork_keep_on_failure {
                    for (dir, _) in &clones {
                        let _ = fs::remove_dir_all(dir);
                    }
                    println!(
                        "Removed all created pane directories under {}.",
                        session_dir.display()
                    );
                } else {
                    println!(
                        "Clones remain under {} for recovery.",
                        session_dir.display()
                    );
                }
                // Update metadata
                let existing: Vec<(PathBuf, String)> = clones
                    .iter()
                    .filter(|(p, _)| p.exists())
                    .map(|(p, b)| (p.clone(), b.clone()))
                    .collect();
                let panes_created = existing.len();
                let pane_dirs_vec: Vec<String> = existing
                    .iter()
                    .map(|(p, _)| p.display().to_string())
                    .collect();
                let branches_vec: Vec<String> = existing.iter().map(|(_, b)| b.clone()).collect();
                let mut meta2 = format!(
                    "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                    created_at,
                    aifo_coder::json_escape(&base_label),
                    aifo_coder::json_escape(&base_ref_or_sha),
                    aifo_coder::json_escape(&base_commit_sha),
                    panes,
                    panes_created,
                    pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                    branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                    aifo_coder::json_escape(&layout)
                );
                if let Some(ref snap) = snapshot_sha {
                    meta2.push_str(&format!(
                        ", \"snapshot_sha\": {}",
                        aifo_coder::json_escape(snap)
                    ));
                }
                meta2.push_str(" }");
                let _ = fs::write(session_dir.join(".meta.json"), meta2);
                return ExitCode::from(1);
            }
        }

        // Panes 2..N
        let mut split_failed = false;
        for (idx, (pane_dir, _b)) in clones.iter().enumerate().skip(1) {
            let i = idx + 1;
            let _pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
            let mut cmd = Command::new(&tmux);
            cmd.arg("split-window")
                .arg("-t")
                .arg(format!("{}:0", &session_name))
                .arg("-c")
                .arg(pane_dir);
            if cli.verbose {
                let target = format!("{}:0", &session_name);
                let preview_split = vec![
                    "tmux".to_string(),
                    "split-window".to_string(),
                    "-t".to_string(),
                    target,
                    "-c".to_string(),
                    pane_dir.display().to_string(),
                ];
                eprintln!(
                    "aifo-coder: tmux: {}",
                    aifo_coder::shell_join(&preview_split)
                );
            }
            let st = cmd.status();
            match st {
                Ok(s) if s.success() => {}
                Ok(_) | Err(_) => {
                    split_failed = true;
                    break;
                }
            }
        }
        if split_failed {
            eprintln!("aifo-coder: tmux split-window failed for one or more panes.");
            // Best-effort: kill the tmux session to avoid leaving a half-configured window
            let mut kill = Command::new(&tmux);
            let _ = kill
                .arg("kill-session")
                .arg("-t")
                .arg(&session_name)
                .status();

            if !cli.fork_keep_on_failure {
                for (dir, _) in &clones {
                    let _ = fs::remove_dir_all(dir);
                }
                println!(
                    "Removed all created pane directories under {}.",
                    session_dir.display()
                );
            } else {
                println!(
                    "Clones remain under {} for recovery.",
                    session_dir.display()
                );
                if let Some((first_dir, first_branch)) = clones.first() {
                    println!("Example recovery:");
                    println!("  git -C \"{}\" status", first_dir.display());
                    println!(
                        "  git -C \"{}\" log --oneline --decorate -n 20",
                        first_dir.display()
                    );
                    println!(
                        "  git -C \"{}\" remote add fork-{}-1 \"{}\"",
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
                }
            }
            // Update metadata with panes_created and existing pane dirs
            let existing: Vec<(PathBuf, String)> = clones
                .iter()
                .filter(|(p, _)| p.exists())
                .map(|(p, b)| (p.clone(), b.clone()))
                .collect();
            let panes_created = existing.len();
            let pane_dirs_vec: Vec<String> = existing
                .iter()
                .map(|(p, _)| p.display().to_string())
                .collect();
            let branches_vec: Vec<String> = existing.iter().map(|(_, b)| b.clone()).collect();
            let mut meta2 = format!(
                "{{ \"created_at\": {}, \"base_label\": {}, \"base_ref_or_sha\": {}, \"base_commit_sha\": {}, \"panes\": {}, \"panes_created\": {}, \"pane_dirs\": [{}], \"branches\": [{}], \"layout\": {}",
                created_at,
                aifo_coder::json_escape(&base_label),
                aifo_coder::json_escape(&base_ref_or_sha),
                aifo_coder::json_escape(&base_commit_sha),
                panes,
                panes_created,
                pane_dirs_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                branches_vec.iter().map(|s| format!("{}", aifo_coder::json_escape(s))).collect::<Vec<_>>().join(", "),
                aifo_coder::json_escape(&layout)
            );
            if let Some(ref snap) = snapshot_sha {
                meta2.push_str(&format!(
                    ", \"snapshot_sha\": {}",
                    aifo_coder::json_escape(snap)
                ));
            }
            meta2.push_str(" }");
            let _ = fs::write(session_dir.join(".meta.json"), meta2);
            return ExitCode::from(1);
        }

        // Layout and options
        let mut lay = Command::new(&tmux);
        lay.arg("select-layout")
            .arg("-t")
            .arg(format!("{}:0", &session_name))
            .arg(&layout_effective);
        if cli.verbose {
            let preview_layout = vec![
                "tmux".to_string(),
                "select-layout".to_string(),
                "-t".to_string(),
                format!("{}:0", &session_name),
                layout_effective.clone(),
            ];
            eprintln!(
                "aifo-coder: tmux: {}",
                aifo_coder::shell_join(&preview_layout)
            );
        }
        let _ = lay.status();

        let mut sync = Command::new(&tmux);
        sync.arg("set-window-option")
            .arg("-t")
            .arg(format!("{}:0", &session_name))
            .arg("synchronize-panes")
            .arg("off");
        if cli.verbose {
            let preview_sync = vec![
                "tmux".to_string(),
                "set-window-option".to_string(),
                "-t".to_string(),
                format!("{}:0", &session_name),
                "synchronize-panes".to_string(),
                "off".to_string(),
            ];
            eprintln!(
                "aifo-coder: tmux: {}",
                aifo_coder::shell_join(&preview_sync)
            );
        }
        let _ = sync.status();

        // Start commands in each pane via tmux send-keys now that the layout is ready
        for (idx, (_pane_dir, _b)) in clones.iter().enumerate() {
            let i = idx + 1;
            let pane_state_dir = state_base.join(&sid).join(format!("pane-{}", i));
            let inner = build_inner(i, &pane_state_dir);
            let script_path = pane_state_dir.join("launch.sh");
            let _ = fs::create_dir_all(&pane_state_dir);
            let _ = fs::write(&script_path, inner.as_bytes());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&script_path, fs::Permissions::from_mode(0o700));
            }
            let target = format!("{}:0.{}", &session_name, idx);
            let shwrap = format!(
                "sh -lc {}",
                aifo_coder::shell_escape(&script_path.display().to_string())
            );
            let mut sk = Command::new(&tmux);
            sk.arg("send-keys")
                .arg("-t")
                .arg(&target)
                .arg(&shwrap)
                .arg("C-m");
            if cli.verbose {
                let preview = vec![
                    "tmux".to_string(),
                    "send-keys".to_string(),
                    "-t".to_string(),
                    target.clone(),
                    shwrap.clone(),
                    "C-m".to_string(),
                ];
                eprintln!("aifo-coder: tmux: {}", aifo_coder::shell_join(&preview));
            }
            let _ = sk.status();
        }

        // Attach or switch
        let attach_cmd = if env::var("TMUX").ok().filter(|s| !s.is_empty()).is_some() {
            vec![
                "switch-client".to_string(),
                "-t".to_string(),
                session_name.clone(),
            ]
        } else {
            vec![
                "attach-session".to_string(),
                "-t".to_string(),
                session_name.clone(),
            ]
        };
        let mut att = Command::new(&tmux);
        for a in &attach_cmd {
            att.arg(a);
        }
        let _ = att.status();

        // After tmux session ends or switch completes, print merging guidance
        println!();
        if use_color_out {
            println!(
                "\x1b[36;1maifo-coder:\x1b[0m fork session \x1b[32;1m{}\x1b[0m completed.",
                sid
            );
        } else {
            println!("aifo-coder: fork session {} completed.", sid);
        }
        println!();
        print_inspect_merge_guidance(
            &repo_root,
            &sid,
            &base_label,
            &base_ref_or_sha,
            &clones,
            use_color_out,
            false,
            true,
        );

        {
            if !matches!(cli.fork_merging_strategy, aifo_coder::MergingStrategy::None) {
                let strat = match cli.fork_merging_strategy {
                    aifo_coder::MergingStrategy::None => "none",
                    aifo_coder::MergingStrategy::Fetch => "fetch",
                    aifo_coder::MergingStrategy::Octopus => "octopus",
                };
                // visual separation from the guidance block above
                println!();
                {
                    let use_err = aifo_coder::color_enabled_stderr();
                    eprintln!(
                        "{}",
                        aifo_coder::paint(
                            use_err,
                            "\x1b[36;1m",
                            &format!("aifo-coder: applying post-fork merge strategy: {}", strat)
                        )
                    );
                }
                match aifo_coder::fork_merge_branches_by_session(
                    &repo_root,
                    &sid,
                    cli.fork_merging_strategy,
                    cli.verbose,
                    cli.dry_run,
                ) {
                    Ok(()) => {
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[32;1m",
                                    &format!("aifo-coder: merge strategy '{}' completed.", strat)
                                )
                            );
                        }
                        if matches!(
                            cli.fork_merging_strategy,
                            aifo_coder::MergingStrategy::Octopus
                        ) && cli.fork_merging_autoclean
                            && !cli.dry_run
                        {
                            eprintln!();
                            {
                                let use_err = aifo_coder::color_enabled_stderr();
                                eprintln!(
                                    "{}",
                                    aifo_coder::paint(
                                        use_err,
                                        "\x1b[36;1m",
                                        &format!(
                                            "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                            sid
                                        )
                                    )
                                );
                            }
                            let opts = aifo_coder::ForkCleanOpts {
                                session: Some(sid.clone()),
                                older_than_days: None,
                                all: false,
                                dry_run: false,
                                yes: true,
                                force: true,
                                keep_dirty: false,
                                json: false,
                            };
                            match aifo_coder::fork_clean(&repo_root, &opts) {
                                Ok(_) => {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[32;1m",
                                            &format!("aifo-coder: disposed fork session {}.", sid)
                                        )
                                    );
                                }
                                Err(e) => {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[33m",
                                            &format!(
                                                "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                sid, e
                                            )
                                        )
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let use_err = aifo_coder::color_enabled_stderr();
                        eprintln!(
                            "{}",
                            aifo_coder::paint(
                                use_err,
                                "\x1b[31;1m",
                                &format!("aifo-coder: merge strategy '{}' failed: {}", strat, e)
                            )
                        );
                    }
                }
            }
            ExitCode::from(0)
        }
    }
}

#[derive(Subcommand, Debug, Clone)]
enum ForkCmd {
    /// List existing fork sessions under the current repo
    List {
        /// Emit machine-readable JSON
        #[arg(long)]
        json: bool,
        /// Scan across repositories under AIFO_CODER_WORKSPACE_ROOT (non-recursive; requires env)
        #[arg(long = "all-repos")]
        all_repos: bool,
        /// Colorize output: auto|always|never
        #[arg(long = "color", value_enum)]
        color: Option<aifo_coder::ColorMode>,
    },
    /// Clean fork sessions and panes with safety protections
    Clean {
        /// Target a single session id
        #[arg(long = "session")]
        session: Option<String>,
        /// Target sessions older than N days
        #[arg(long = "older-than")]
        older_than: Option<u64>,
        /// Target all sessions
        #[arg(long = "all")]
        all: bool,
        /// Print what would be done without deleting
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Proceed without interactive confirmation
        #[arg(long = "yes")]
        yes: bool,
        /// Override safety protections and delete everything
        #[arg(long = "force")]
        force: bool,
        /// Delete only clean panes; keep dirty/ahead/base-unknown
        #[arg(long = "keep-dirty")]
        keep_dirty: bool,
        /// Emit machine-readable JSON summary (plan in --dry-run; result when executed)
        #[arg(long)]
        json: bool,
    },

    /// Merge fork panes back into the original repository
    Merge {
        /// Session id to merge
        #[arg(long = "session")]
        session: String,
        /// Strategy: none|fetch|octopus
        #[arg(long = "strategy", value_enum)]
        strategy: aifo_coder::MergingStrategy,
        /// Automatically dispose the fork session after a successful octopus merge
        #[arg(long = "autoclean")]
        autoclean: bool,
        /// Print what would be done without modifying
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum Agent {
    /// Run diagnostics to check environment and configuration
    Doctor,

    /// Show effective image references (including flavor/registry)
    Images,

    /// Clear on-disk caches (e.g., registry probe cache)
    CacheClear,

    /// Purge all named toolchain cache volumes (cargo, npm, pip, ccache, go)
    ToolchainCacheClear,

    /// Toolchain sidecar: run a command inside a language toolchain sidecar
    Toolchain {
        #[arg(value_enum)]
        kind: ToolchainKind,
        /// Override the toolchain image reference for this run
        #[arg(long = "toolchain-image")]
        image: Option<String>,
        /// Disable named cache volumes for the toolchain sidecar
        #[arg(long = "no-toolchain-cache")]
        no_cache: bool,
        /// Command and arguments to execute inside the sidecar (after --)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Run OpenAI Codex CLI
    Codex {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run Charmbracelet Crush
    Crush {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    /// Run Aider
    Aider {
        /// Additional arguments passed through to the agent
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Fork maintenance commands
    #[command(
        after_long_help = "Examples:\n  aifo-coder fork list --json\n  aifo-coder fork clean --session abc123 --dry-run --json\n  aifo-coder fork clean --older-than 30 --yes --keep-dirty\n  aifo-coder fork merge --session abc123 --strategy octopus --autoclean\n"
    )]
    Fork {
        #[command(subcommand)]
        cmd: ForkCmd,
    },
}

struct OutputNewlineGuard;

impl Drop for OutputNewlineGuard {
    fn drop(&mut self) {
        // Ensure a trailing blank line on stdout at process end
        println!();
    }
}

fn main() -> ExitCode {
    // Leading blank line at program start
    println!();
    let _aifo_output_newline_guard = OutputNewlineGuard;
    // Load environment variables from .env if present (no error if missing)
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    // Configure color mode as early as possible (only when explicitly provided on CLI)
    if let Some(mode) = cli.color {
        aifo_coder::set_color_mode(mode);
    }

    // Optional: invalidate on-disk registry cache before any probes
    if cli.invalidate_registry_cache {
        aifo_coder::invalidate_registry_cache();
    }

    // Apply CLI flavor override by setting the environment variable the launcher uses
    if let Some(flavor) = cli.flavor {
        match flavor {
            Flavor::Full => std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "full"),
            Flavor::Slim => std::env::set_var("AIFO_CODER_IMAGE_FLAVOR", "slim"),
        }
    }

    // Fork orchestrator (Phase 3): run early if requested
    if let Some(n) = cli.fork {
        if n >= 2 {
            return fork_run(&cli, n);
        }
    }
    // Optional auto-clean of stale fork sessions and stale notice (Phase 6)
    // Suppress stale notice here when running 'doctor' (doctor prints its own notice).
    if !matches!(cli.command, Agent::Fork { .. }) && !matches!(cli.command, Agent::Doctor) {
        aifo_coder::fork_autoclean_if_enabled();
        // Stale sessions notice (Phase 6): print suggestions for old fork sessions on normal runs
        aifo_coder::fork_print_stale_notice();
    }

    // Fork maintenance subcommands (Phase 6): operate without starting agents or acquiring locks
    if let Agent::Fork { cmd } = &cli.command {
        match cmd {
            ForkCmd::List {
                json,
                all_repos,
                color,
            } => {
                if let Some(mode) = color {
                    aifo_coder::set_color_mode(*mode);
                }
                if *all_repos {
                    // In all-repos mode, do not require being inside a Git repo; workspace root is taken from env
                    let dummy = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                    let code = aifo_coder::fork_list(&dummy, *json, true).unwrap_or(1);
                    return ExitCode::from(code as u8);
                } else {
                    let repo_root = match aifo_coder::repo_root() {
                        Some(p) => p,
                        None => {
                            eprintln!("aifo-coder: error: fork maintenance commands must be run inside a Git repository.");
                            return ExitCode::from(1);
                        }
                    };
                    let code = aifo_coder::fork_list(&repo_root, *json, false).unwrap_or(1);
                    return ExitCode::from(code as u8);
                }
            }
            ForkCmd::Clean {
                session,
                older_than,
                all,
                dry_run,
                yes,
                force,
                keep_dirty,
                json,
            } => {
                let repo_root = match aifo_coder::repo_root() {
                    Some(p) => p,
                    None => {
                        eprintln!("aifo-coder: error: fork maintenance commands must be run inside a Git repository.");
                        return ExitCode::from(1);
                    }
                };
                let opts = aifo_coder::ForkCleanOpts {
                    session: session.clone(),
                    older_than_days: *older_than,
                    all: *all,
                    dry_run: *dry_run,
                    yes: *yes,
                    force: *force,
                    keep_dirty: *keep_dirty,
                    json: *json,
                };
                let code = aifo_coder::fork_clean(&repo_root, &opts).unwrap_or(1);
                return ExitCode::from(code as u8);
            }
            ForkCmd::Merge {
                session,
                strategy,
                autoclean,
                dry_run,
            } => {
                let repo_root = match aifo_coder::repo_root() {
                    Some(p) => p,
                    None => {
                        eprintln!("aifo-coder: error: fork maintenance commands must be run inside a Git repository.");
                        return ExitCode::from(1);
                    }
                };
                match aifo_coder::fork_merge_branches_by_session(
                    &repo_root,
                    session,
                    *strategy,
                    cli.verbose,
                    *dry_run,
                ) {
                    Ok(()) => {
                        if matches!(strategy, aifo_coder::MergingStrategy::Octopus)
                            && *autoclean
                            && !*dry_run
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[36;1m",
                                    &format!(
                                        "aifo-coder: octopus merge succeeded; disposing fork session {} ...",
                                        session
                                    )
                                )
                            );
                            let opts = aifo_coder::ForkCleanOpts {
                                session: Some(session.clone()),
                                older_than_days: None,
                                all: false,
                                dry_run: false,
                                yes: true,
                                force: true,
                                keep_dirty: false,
                                json: false,
                            };
                            match aifo_coder::fork_clean(&repo_root, &opts) {
                                Ok(_) => {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[32;1m",
                                            &format!(
                                                "aifo-coder: disposed fork session {}.",
                                                session
                                            )
                                        )
                                    );
                                }
                                Err(e) => {
                                    let use_err = aifo_coder::color_enabled_stderr();
                                    eprintln!(
                                        "{}",
                                        aifo_coder::paint(
                                            use_err,
                                            "\x1b[33m",
                                            &format!(
                                                "aifo-coder: warning: failed to dispose fork session {}: {}",
                                                session, e
                                            )
                                        )
                                    );
                                }
                            }
                        }
                        return ExitCode::from(0);
                    }
                    Err(e) => {
                        {
                            let use_err = aifo_coder::color_enabled_stderr();
                            eprintln!(
                                "{}",
                                aifo_coder::paint(
                                    use_err,
                                    "\x1b[31;1m",
                                    &format!("aifo-coder: fork merge failed: {}", e)
                                )
                            );
                        }
                        return ExitCode::from(1);
                    }
                }
            }
        }
    }

    // Doctor subcommand runs diagnostics without acquiring a lock
    if let Agent::Doctor = &cli.command {
        print_startup_banner();
        let _ = warn_if_tmp_workspace(false);
        run_doctor(cli.verbose);
        return ExitCode::from(0);
    } else if let Agent::Images = &cli.command {
        print_startup_banner();
        let _ = warn_if_tmp_workspace(false);
        eprintln!("aifo-coder images");
        eprintln!();

        // Flavor and registry display
        let flavor_env = std::env::var("AIFO_CODER_IMAGE_FLAVOR").unwrap_or_default();
        let flavor = if flavor_env.trim().eq_ignore_ascii_case("slim") {
            "slim"
        } else {
            "full"
        };
        let rp = aifo_coder::preferred_registry_prefix_quiet();
        let reg_display = if rp.is_empty() {
            "Docker Hub".to_string()
        } else {
            rp.trim_end_matches('/').to_string()
        };

        let use_color = atty::is(atty::Stream::Stderr);
        let flavor_val = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", flavor)
        } else {
            flavor.to_string()
        };
        let reg_val = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", reg_display)
        } else {
            reg_display
        };

        eprintln!("  flavor:   {}", flavor_val);
        eprintln!("  registry: {}", reg_val);
        eprintln!();

        // Effective image references
        let codex_img = default_image_for("codex");
        let crush_img = default_image_for("crush");
        let aider_img = default_image_for("aider");
        let codex_val = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", codex_img)
        } else {
            codex_img
        };
        let crush_val = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", crush_img)
        } else {
            crush_img
        };
        let aider_val = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", aider_img)
        } else {
            aider_img
        };
        eprintln!("  codex: {}", codex_val);
        eprintln!("  crush: {}", crush_val);
        eprintln!("  aider: {}", aider_val);
        eprintln!();

        return ExitCode::from(0);
    } else if let Agent::CacheClear = &cli.command {
        aifo_coder::invalidate_registry_cache();
        eprintln!("aifo-coder: cleared on-disk registry cache.");
        return ExitCode::from(0);
    } else if let Agent::ToolchainCacheClear = &cli.command {
        print_startup_banner();
        let _ = warn_if_tmp_workspace(false);
        match aifo_coder::toolchain_purge_caches(cli.verbose) {
            Ok(()) => {
                eprintln!("aifo-coder: purged toolchain cache volumes.");
                return ExitCode::from(0);
            }
            Err(e) => {
                eprintln!("aifo-coder: failed to purge toolchain caches: {}", e);
                return ExitCode::from(1);
            }
        }
    } else if let Agent::Toolchain {
        kind,
        image,
        no_cache,
        args,
    } = &cli.command
    {
        print_startup_banner();
        if !warn_if_tmp_workspace(true) {
            eprintln!("aborted.");
            return ExitCode::from(1);
        }
        if cli.verbose {
            eprintln!("aifo-coder: toolchain kind: {}", kind.as_str());
            if let Some(img) = image.as_deref() {
                eprintln!("aifo-coder: toolchain image override: {}", img);
            }
            if *no_cache {
                eprintln!("aifo-coder: toolchain caches disabled for this run");
            }
        }
        if cli.dry_run {
            let _ = aifo_coder::toolchain_run(
                kind.as_str(),
                args,
                image.as_deref(),
                *no_cache,
                true,
                true,
            );
            return ExitCode::from(0);
        }
        let code = match aifo_coder::toolchain_run(
            kind.as_str(),
            args,
            image.as_deref(),
            *no_cache,
            cli.verbose,
            false,
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                if e.kind() == io::ErrorKind::NotFound {
                    127
                } else {
                    1
                }
            }
        };
        return ExitCode::from((code & 0xff) as u8);
    }

    // Build docker command and run it
    let (agent, args) = match &cli.command {
        Agent::Codex { args } => ("codex", args.clone()),
        Agent::Crush { args } => ("crush", args.clone()),
        Agent::Aider { args } => ("aider", args.clone()),
        Agent::Doctor => {
            unreachable!("Doctor subcommand is handled earlier and returns immediately")
        }
        Agent::Images => {
            unreachable!("Images subcommand is handled earlier and returns immediately")
        }
        Agent::CacheClear => {
            unreachable!("CacheClear subcommand is handled earlier and returns immediately")
        }
        Agent::ToolchainCacheClear => unreachable!(
            "ToolchainCacheClear subcommand is handled earlier and returns immediately"
        ),
        Agent::Toolchain { .. } => {
            unreachable!("Toolchain subcommand is handled earlier and returns immediately")
        }
        Agent::Fork { .. } => {
            unreachable!("Fork maintenance subcommands are handled earlier and return immediately")
        }
    };

    // Print startup banner before any further diagnostics
    print_startup_banner();
    maybe_warn_missing_toolchain_agent(&cli, agent);
    if !warn_if_tmp_workspace(true) {
        eprintln!("aborted.");
        return ExitCode::from(1);
    }

    // Phase 2: if toolchains were requested, prepare shims, start sidecars and proxy
    let mut tc_session_id: Option<String> = None;
    let mut tc_proxy_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>> = None;
    let mut tc_proxy_handle: Option<std::thread::JoinHandle<()>> = None;

    if !cli.toolchain.is_empty() || !cli.toolchain_spec.is_empty() {
        // kinds as strings (from enum flag)
        let mut kinds: Vec<String> = cli
            .toolchain
            .iter()
            .map(|k| k.as_str().to_string())
            .collect();

        // Parse spec strings kind[@version]
        fn parse_spec(s: &str) -> (String, Option<String>) {
            let t = s.trim();
            if let Some((k, v)) = t.split_once('@') {
                (k.trim().to_string(), Some(v.trim().to_string()))
            } else {
                (t.to_string(), None)
            }
        }
        let mut spec_versions: Vec<(String, String)> = Vec::new();
        for s in &cli.toolchain_spec {
            let (k, v) = parse_spec(s);
            if !k.is_empty() {
                kinds.push(k.clone());
                if let Some(ver) = v {
                    spec_versions.push((k, ver));
                }
            }
        }
        // Normalize kinds and dedup
        use std::collections::BTreeSet;
        let mut set = BTreeSet::new();
        let mut kinds_norm: Vec<String> = Vec::new();
        for k in kinds {
            let norm = aifo_coder::normalize_toolchain_kind(&k);
            if set.insert(norm.clone()) {
                kinds_norm.push(norm);
            }
        }
        let kinds = kinds_norm;

        // parse overrides kind=image
        let mut overrides: Vec<(String, String)> = Vec::new();
        for s in &cli.toolchain_image {
            if let Some((k, v)) = s.split_once('=') {
                if !k.trim().is_empty() && !v.trim().is_empty() {
                    overrides.push((
                        aifo_coder::normalize_toolchain_kind(k),
                        v.trim().to_string(),
                    ));
                }
            }
        }
        // Add overrides derived from versions unless already overridden
        for (k, ver) in spec_versions {
            let kind = aifo_coder::normalize_toolchain_kind(&k);
            if !overrides.iter().any(|(kk, _)| kk == &kind) {
                let img = aifo_coder::default_toolchain_image_for_version(&kind, &ver);
                overrides.push((kind, img));
            }
        }

        if cli.dry_run {
            if cli.verbose {
                eprintln!("aifo-coder: would attach toolchains: {:?}", kinds);
                if !overrides.is_empty() {
                    eprintln!("aifo-coder: would use image overrides: {:?}", overrides);
                }
                if cli.no_toolchain_cache {
                    eprintln!("aifo-coder: would disable toolchain caches");
                }
                if cfg!(target_os = "linux") && cli.toolchain_unix_socket {
                    eprintln!("aifo-coder: would use unix:/// socket transport for proxy and mount /run/aifo");
                }
                if !cli.toolchain_bootstrap.is_empty() {
                    eprintln!("aifo-coder: would bootstrap: {:?}", cli.toolchain_bootstrap);
                }
                eprintln!("aifo-coder: would prepare and mount /opt/aifo/bin shims; set AIFO_TOOLEEXEC_URL/TOKEN; join aifo-net-<id>");
            }
        } else {
            // Phase 3: use embedded shims in the agent image; host override via AIFO_SHIM_DIR still supported
            if cli.verbose {
                eprintln!("aifo-coder: using embedded PATH shims from agent image (/opt/aifo/bin)");
            }
            // Optional: switch to unix socket transport for proxy on Linux
            #[cfg(target_os = "linux")]
            if cli.toolchain_unix_socket {
                std::env::set_var("AIFO_TOOLEEXEC_USE_UNIX", "1");
            }

            // Start sidecars
            match aifo_coder::toolchain_start_session(
                &kinds,
                &overrides,
                cli.no_toolchain_cache,
                cli.verbose,
            ) {
                Ok(sid) => {
                    // Set network env for agent container to join
                    let net = format!("aifo-net-{}", sid);
                    std::env::set_var("AIFO_SESSION_NETWORK", &net);
                    #[cfg(target_os = "linux")]
                    {
                        // Ensure agent can reach host proxy when using TCP; not needed for unix socket transport
                        if !cli.toolchain_unix_socket {
                            std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");
                        }
                    }
                    tc_session_id = Some(sid);
                }
                Err(e) => {
                    eprintln!("aifo-coder: failed to start toolchain sidecars: {}", e);
                    return ExitCode::from(1);
                }
            }

            // Bootstrap (e.g., typescript=global) before starting proxy
            if let Some(ref sid) = tc_session_id {
                if !cli.toolchain_bootstrap.is_empty() {
                    let want_ts_global = cli.toolchain_bootstrap.iter().any(|b| {
                        let t = b.trim().to_ascii_lowercase();
                        t == "typescript=global" || t == "ts=global"
                    });
                    if want_ts_global && kinds.iter().any(|k| k == "node") {
                        if let Err(e) =
                            aifo_coder::toolchain_bootstrap_typescript_global(sid, cli.verbose)
                        {
                            eprintln!("aifo-coder: typescript bootstrap failed: {}", e);
                        }
                    }
                }
            }

            // Start proxy
            if let Some(ref sid) = tc_session_id {
                match aifo_coder::toolexec_start_proxy(sid, cli.verbose) {
                    Ok((url, token, flag, handle)) => {
                        std::env::set_var("AIFO_TOOLEEXEC_URL", &url);
                        std::env::set_var("AIFO_TOOLEEXEC_TOKEN", &token);
                        tc_proxy_flag = Some(flag);
                        tc_proxy_handle = Some(handle);
                    }
                    Err(e) => {
                        eprintln!("aifo-coder: failed to start toolexec proxy: {}", e);
                        if let Some(s) = tc_session_id.as_deref() {
                            aifo_coder::toolchain_cleanup_session(s, cli.verbose);
                        }
                        return ExitCode::from(1);
                    }
                }
            }
        }
    }

    let image = cli
        .image
        .clone()
        .unwrap_or_else(|| default_image_for(agent));

    println!();

    let apparmor_profile = desired_apparmor_profile();
    match build_docker_cmd(agent, &args, &image, apparmor_profile.as_deref()) {
        Ok((mut cmd, preview)) => {
            if cli.verbose {
                eprintln!(
                    "aifo-coder: effective apparmor profile: {}",
                    apparmor_profile.as_deref().unwrap_or("(disabled)")
                );
                // Show chosen registry and source for transparency
                let rp = aifo_coder::preferred_registry_prefix_quiet();
                let reg_display = if rp.is_empty() {
                    "Docker Hub".to_string()
                } else {
                    rp.trim_end_matches('/').to_string()
                };
                let reg_src = aifo_coder::preferred_registry_source();
                eprintln!("aifo-coder: registry: {reg_display} (source: {reg_src})");
                eprintln!("aifo-coder: image: {image}");
                eprintln!("aifo-coder: agent: {agent}");
            }
            if cli.verbose || cli.dry_run {
                eprintln!("aifo-coder: docker: {preview}");
            }
            if cli.dry_run {
                eprintln!("aifo-coder: dry-run requested; not executing Docker.");
                return ExitCode::from(0);
            }
            // Acquire lock only for real execution; honor AIFO_CODER_SKIP_LOCK=1 for child panes
            let skip_lock = std::env::var("AIFO_CODER_SKIP_LOCK").ok().as_deref() == Some("1");
            let maybe_lock = if skip_lock {
                None
            } else {
                match acquire_lock() {
                    Ok(f) => Some(f),
                    Err(e) => {
                        eprintln!("{e}");
                        return ExitCode::from(1);
                    }
                }
            };
            let status = cmd.status().expect("failed to start docker");
            // Release lock before exiting (if held)
            if let Some(lock) = maybe_lock {
                drop(lock);
            }

            // Phase 2 cleanup (if toolchain shims/proxy were attached)
            if let Some(flag) = tc_proxy_flag.take() {
                flag.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            if let Some(h) = tc_proxy_handle.take() {
                let _ = h.join();
            }
            if let Some(ref sid) = tc_session_id {
                // In fork panes, sidecars may be shared across panes; defer cleanup to user (fork clean)
                if std::env::var("AIFO_CODER_FORK_SESSION")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .is_none()
                {
                    aifo_coder::toolchain_cleanup_session(sid, cli.verbose);
                }
            }

            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(e) => {
            eprintln!("{e}");
            // Phase 2 cleanup on error
            if let Some(flag) = tc_proxy_flag.take() {
                flag.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            if let Some(h) = tc_proxy_handle.take() {
                let _ = h.join();
            }
            if let Some(ref sid) = tc_session_id {
                // In fork panes, sidecars may be shared across panes; defer cleanup to user (fork clean)
                if std::env::var("AIFO_CODER_FORK_SESSION")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .is_none()
                {
                    aifo_coder::toolchain_cleanup_session(sid, cli.verbose);
                }
            }
            if e.kind() == io::ErrorKind::NotFound {
                return ExitCode::from(127);
            }
            ExitCode::from(1)
        }
    }
}

fn default_image_for(agent: &str) -> String {
    if let Ok(img) = env::var("AIFO_CODER_IMAGE") {
        if !img.trim().is_empty() {
            return img;
        }
    }
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let image_name = format!("{name_prefix}-{agent}{suffix}:{tag}");
    let registry = preferred_registry_prefix();
    if registry.is_empty() {
        image_name
    } else {
        format!("{registry}{image_name}")
    }
}

fn default_image_for_quiet(agent: &str) -> String {
    if let Ok(img) = env::var("AIFO_CODER_IMAGE") {
        if !img.trim().is_empty() {
            return img;
        }
    }
    let name_prefix =
        env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let suffix = match env::var("AIFO_CODER_IMAGE_FLAVOR") {
        Ok(v) if v.trim().eq_ignore_ascii_case("slim") => "-slim",
        _ => "",
    };
    let image_name = format!("{name_prefix}-{agent}{suffix}:{tag}");
    let registry = aifo_coder::preferred_registry_prefix_quiet();
    if registry.is_empty() {
        image_name
    } else {
        format!("{registry}{image_name}")
    }
}
