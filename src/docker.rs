#![allow(clippy::module_name_repetitions)]
//! Docker command construction and runtime detection.

use crate::ensure_file_exists;
use crate::path_pair;
#[cfg(unix)]
use nix::unistd::{getgid, getuid};
use once_cell::sync::Lazy;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use which::which;

#[cfg(feature = "otel")]
use tracing::instrument;

// Pass-through environment variables to the containerized agent
static PASS_ENV_VARS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        // AIFO master env (single source of truth)
        "AIFO_API_KEY",
        "AIFO_API_BASE",
        "AIFO_API_VERSION",
        // Git author/committer overrides
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        // GPG signing controls
        "AIFO_CODER_GIT_SIGN",
        "GIT_SIGNING_KEY",
        // Timezone
        "TZ",
        // Editor preferences
        "EDITOR",
        "VISUAL",
        "TERM",
    ]
});

pub fn container_runtime_path() -> io::Result<PathBuf> {
    // Allow tests or callers to explicitly disable Docker detection to avoid hard failures
    if env::var("AIFO_CODER_TEST_DISABLE_DOCKER").ok().as_deref() == Some("1")
        || env::var("AIFO_CODER_SKIP_DOCKER").ok().as_deref() == Some("1")
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Docker disabled by environment override.",
        ));
    }

    if let Ok(p) = which("docker") {
        return Ok(p);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Docker is required but was not found in PATH.",
    ))
}

fn agent_bin_and_path(agent: &str) -> (String, String) {
    let abs = match agent {
        "aider" => "/opt/venv/bin/aider",
        "codex" => "/usr/local/bin/codex",
        "crush" => "/usr/local/bin/crush",
        "openhands" => "/opt/venv-openhands/bin/openhands",
        "opencode" => "/usr/local/bin/opencode",
        "plandex" => "/usr/local/bin/plandex",
        _ => agent,
    }
    .to_string();

    let path = match agent {
        "aider" => "/opt/aifo/bin:/opt/venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH",
        "codex" | "crush" => "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/aifo/bin:$PATH",
        _ => "/opt/aifo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH",
    }
    .to_string();

    (abs, path)
}

fn collect_env_flags(agent: &str, uid_opt: Option<u32>) -> Vec<OsString> {
    let mut env_flags: Vec<OsString> = Vec::new();

    // Pass-through env
    for var in PASS_ENV_VARS.iter().copied() {
        if let Ok(val) = env::var(var) {
            if !val.is_empty() {
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from(var));
            }
        }
    }

    // Fixed environment
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("HOME=/home/coder"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("USER=coder"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("CODEX_HOME=/home/coder/.codex"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("GNUPGHOME=/home/coder/.gnupg"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("SHELL=/opt/aifo/bin/sh"));

    // Phase 1: Config clone policy envs (entrypoint will perform the copy)
    // Always set in-container host config mount path explicitly.
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from(
        "AIFO_CONFIG_HOST_DIR=/home/coder/.aifo-config-host",
    ));
    // Back-compat for images expecting AIFO_CODER_CONFIG_HOST_DIR
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from(
        "AIFO_CODER_CONFIG_HOST_DIR=/home/coder/.aifo-config-host",
    ));
    // Optional policy knobs: pass through when set on host.
    if let Ok(v) = env::var("AIFO_CONFIG_ENABLE") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_CONFIG_ENABLE={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_CONFIG_MAX_SIZE") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_CONFIG_MAX_SIZE={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_CONFIG_ALLOW_EXT") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_CONFIG_ALLOW_EXT={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_CONFIG_SECRET_HINTS") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_CONFIG_SECRET_HINTS={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_CONFIG_COPY_ALWAYS") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_CONFIG_COPY_ALWAYS={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_CONFIG_DST_DIR") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_CONFIG_DST_DIR={v}")));
        }
    }

    // XDG_RUNTIME_DIR (unix only)
    if let Some(uid) = uid_opt {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from(format!(
            "XDG_RUNTIME_DIR=/tmp/runtime-{}",
            uid
        )));
    }

    // Pinentry TTY
    if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from("GPG_TTY=/dev/tty"));
    }

    // Unified AIFO_* → OpenAI/Azure mappings
    if let Ok(v) = env::var("AIFO_API_KEY") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_KEY={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_API_KEY={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_KEY={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_API_BASE") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_BASE_URL={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_BASE={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_ENDPOINT={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_BASE={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from("OPENAI_API_TYPE=azure"));
        }
    }
    if let Ok(v) = env::var("AIFO_API_VERSION") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_VERSION={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_API_VERSION={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_VERSION={v}")));
            // Ensure litellm/OpenHands use the same Azure Responses API version
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("LITELLM_AZURE_API_VERSION={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!(
                "AZURE_OPENAI_RESPONSES_API_VERSION={v}"
            )));
        }
    }
    if let Ok(v) = env::var("AIFO_TOOLEEXEC_URL") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_TOOLEEXEC_URL={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_TOOLEEXEC_TOKEN") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_TOOLEEXEC_TOKEN={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_TOOLCHAIN_VERBOSE") {
        if !v.is_empty() {
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AIFO_TOOLCHAIN_VERBOSE={v}")));
        }
    }

    // Disable commit signing for Aider
    if agent == "aider" {
        if let Ok(v) = env::var("AIFO_CODER_GIT_SIGN") {
            let vl = v.to_ascii_lowercase();
            if ["0", "false", "no", "off"].contains(&vl.as_str()) {
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from("GIT_CONFIG_COUNT=1"));
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from("GIT_CONFIG_KEY_0=commit.gpgsign"));
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from("GIT_CONFIG_VALUE_0=false"));
            }
        }
    }

    env_flags
}

fn host_claude_config_path(host_home: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Ok(appdata) = env::var("APPDATA") {
            if !appdata.trim().is_empty() {
                return Some(
                    PathBuf::from(appdata)
                        .join("Claude")
                        .join("claude_desktop_config.json"),
                );
            }
        }
        // Fallback: derive from host_home if APPDATA is missing
        return Some(
            host_home
                .join("AppData")
                .join("Roaming")
                .join("Claude")
                .join("claude_desktop_config.json"),
        );
    }
    #[cfg(target_os = "macos")]
    {
        Some(
            host_home
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
        )
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Some(
            host_home
                .join(".config")
                .join("claude")
                .join("claude_desktop_config.json"),
        )
    }
}

fn collect_volume_flags(agent: &str, host_home: &Path, pwd: &Path) -> Vec<OsString> {
    let mut volume_flags: Vec<OsString> = Vec::new();

    // Transparent host-side auto-migration of legacy Aider and other agent config files into
    // standardized config dirs under ~/.config/aifo-coder/<agent>-PID so aifo-entrypoint can
    // bridge them inside the container without mounting large state/caches.
    //
    // Aider keeps its own special-case block for dotfiles; other agents use the generic
    // per-agent staging below.
    {
        // Always stage latest Aider dotfiles into a canonical per-agent directory (~/.config/aifo-coder/aider)
        let legacy_names = [
            ".aider.conf.yml",
            ".aider.model.settings.yml",
            ".aider.model.metadata.json",
        ];
        let cfg_root = host_home.join(".config").join("aifo-coder");
        let _ = fs::create_dir_all(&cfg_root);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&cfg_root, fs::Permissions::from_mode(0o700));
        }
        let staging = cfg_root.join("aider");
        let _ = fs::create_dir_all(&staging);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&staging, fs::Permissions::from_mode(0o700));
        }
        let max_sz = env::var("AIFO_CONFIG_MAX_SIZE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(262_144);

        let mut staged_any = false;
        for name in &legacy_names {
            let src = host_home.join(name);
            if src.is_file() {
                if let Ok(md) = fs::metadata(&src) {
                    if md.len() <= max_sz {
                        let dst = staging.join(name);
                        let _ = fs::copy(&src, &dst);
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            // Treat Aider configs as potentially sensitive; default to 0600
                            let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o600));
                        }
                        staged_any = true;
                    }
                }
            }
        }
        if staged_any {
            // Track staged dir for cleanup and overlay-mount it to expected container path
            let mut staged = env::var("AIFO_CONFIG_STAGING_DIRS").unwrap_or_default();
            if !staged.is_empty() {
                staged.push(':');
            }
            staged.push_str(&staging.to_string_lossy());
            env::set_var("AIFO_CONFIG_STAGING_DIRS", staged);

            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!(
                "{}:/home/coder/.aifo-config-host/aider:ro",
                staging.display()
            )));
        }
    }

    // Per-agent small config staging (top-level regular files; whitelisted ext/size).
    {
        let cfg_root = host_home.join(".config").join("aifo-coder");
        let _ = fs::create_dir_all(&cfg_root);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&cfg_root, fs::Permissions::from_mode(0o700));
        }
        let max_sz = env::var("AIFO_CONFIG_MAX_SIZE")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(262_144);
        let exts_env = env::var("AIFO_CONFIG_ALLOW_EXT")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "json,toml,yaml,yml,ini,conf,crt,pem,key,token".to_string());
        let allowed_exts: Vec<String> = exts_env
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        fn should_keep_file(
            path: &Path,
            max_sz: u64,
            allowed_exts: &[String],
            verbose: bool,
            agent: &str,
        ) -> bool {
            let name = match path.file_name().and_then(|s| s.to_str()) {
                Some(n) => n,
                None => return false,
            };
            // Reject names with suspicious characters
            if name.is_empty()
                || name.chars().any(|c| {
                    !c.is_ascii() || (!c.is_alphanumeric() && !['.', '-', '_'].contains(&c))
                })
            {
                if verbose {
                    eprintln!(
                        "aifo-entrypoint: config: skip invalid name for agent {}: {}",
                        agent, name
                    );
                }
                return false;
            }
            let md = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => return false,
            };
            if !md.is_file() || md.len() > max_sz {
                return false;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            if !allowed_exts.contains(&ext) {
                return false;
            }
            true
        }

        fn stage_top_level_files(
            agent: &str,
            src_dir: &Path,
            cfg_root: &Path,
            max_sz: u64,
            allowed_exts: &[String],
        ) -> Option<PathBuf> {
            if !src_dir.is_dir() {
                return None;
            }
            // Use a stable per-agent directory name so entrypoint can consume CFG_DST/<agent>
            let staging = cfg_root.join(agent);
            let _ = fs::create_dir_all(&staging);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&staging, fs::Permissions::from_mode(0o700));
            }
            let verbose = env::var("AIFO_TOOLCHAIN_VERBOSE").ok().as_deref() == Some("1");
            let mut staged_any = false;
            if let Ok(rd) = fs::read_dir(src_dir) {
                for ent in rd.flatten() {
                    let p = ent.path();
                    if !should_keep_file(&p, max_sz, allowed_exts, verbose, agent) {
                        continue;
                    }
                    let name = match p.file_name() {
                        Some(n) => n,
                        None => continue,
                    };
                    let dst = staging.join(name);
                    let _ = fs::copy(&p, &dst);
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = fs::set_permissions(&dst, fs::Permissions::from_mode(0o600));
                    }
                    staged_any = true;
                }
            }
            if staged_any {
                Some(staging)
            } else {
                None
            }
        }

        let mut staged_dirs: Vec<PathBuf> = Vec::new();

        match agent {
            "crush" => {
                let src = host_home.join(".crush");
                if let Some(p) =
                    stage_top_level_files("crush", &src, &cfg_root, max_sz, &allowed_exts)
                {
                    staged_dirs.push(p);
                }
            }
            "openhands" => {
                let src = host_home.join(".openhands");
                if let Some(p) =
                    stage_top_level_files("openhands", &src, &cfg_root, max_sz, &allowed_exts)
                {
                    staged_dirs.push(p);
                }
            }
            "opencode" => {
                let src = host_home.join(".config").join("opencode");
                if let Some(p) =
                    stage_top_level_files("opencode", &src, &cfg_root, max_sz, &allowed_exts)
                {
                    staged_dirs.push(p);
                }
            }
            "plandex" => {
                let src = host_home.join(".plandex-home");
                if let Some(p) =
                    stage_top_level_files("plandex", &src, &cfg_root, max_sz, &allowed_exts)
                {
                    staged_dirs.push(p);
                }
            }
            _ => {}
        }

        if !staged_dirs.is_empty() {
            let mut staged_env = env::var("AIFO_CONFIG_STAGING_DIRS").unwrap_or_default();
            for dir in &staged_dirs {
                if !staged_env.is_empty() {
                    staged_env.push(':');
                }
                staged_env.push_str(&dir.to_string_lossy());
            }
            env::set_var("AIFO_CONFIG_STAGING_DIRS", staged_env);

            for dir in &staged_dirs {
                let sub = match agent {
                    "codex" => "codex",
                    "crush" => "crush",
                    "openhands" => "openhands",
                    "opencode" => "opencode",
                    "plandex" => "plandex",
                    _ => continue,
                };
                volume_flags.push(OsString::from("-v"));
                volume_flags.push(OsString::from(format!(
                    "{}:/home/coder/.aifo-config-host/{}:ro",
                    dir.display(),
                    sub
                )));
            }
        }
    }

    // Fork-state mounts (when enabled) or HOME-based mounts.
    // When AIFO_CODER_FORK_STATE_DIR is non-empty, use repo-scoped fork state roots exclusively.
    // Otherwise, always fall back to HOME-based mounts regardless of config staging.
    if let Ok(state_dir) = env::var("AIFO_CODER_FORK_STATE_DIR") {
        let sd = state_dir.trim();
        if !sd.is_empty() {
            let base = PathBuf::from(sd);
            let mut pairs: Vec<(PathBuf, &str)> = vec![
                (base.join(".aider"), "/home/coder/.aider"),
                (base.join(".codex"), "/home/coder/.codex"),
                (base.join(".crush"), "/home/coder/.crush"),
                (base.join(".local_state"), "/home/coder/.local/state"),
            ];
            if agent == "opencode" {
                pairs.push((base.join(".opencode"), "/home/coder/.local/share/opencode"));
                pairs.push((
                    base.join(".opencode_config"),
                    "/home/coder/.config/opencode",
                ));
                pairs.push((base.join(".opencode_cache"), "/home/coder/.cache/opencode"));
            }
            if agent == "openhands" {
                pairs.push((base.join(".openhands"), "/home/coder/.openhands"));
            }
            if agent == "plandex" {
                pairs.push((base.join(".plandex-home"), "/home/coder/.plandex-home"));
            }
            for (src, dst) in pairs {
                let _ = fs::create_dir_all(&src);
                volume_flags.push(OsString::from("-v"));
                volume_flags.push(path_pair(&src, dst));
            }
            // When using fork state, skip HOME-based mounts entirely.
            return volume_flags;
        }
    }

    {
        // HOME-based mounts
        let crush_dir = host_home.join(".local").join("share").join("crush");
        #[cfg(windows)]
        let opencode_share = env::var("LOCALAPPDATA")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| host_home.join(".local").join("share"))
            .join("opencode");
        #[cfg(not(windows))]
        let opencode_share = host_home.join(".local").join("share").join("opencode");
        let local_state_dir = host_home.join(".local").join("state");
        let crush_state_dir = host_home.join(".crush");
        let codex_dir = host_home.join(".codex");
        let aider_dir = host_home.join(".aider");

        {
            let mut base_dirs: Vec<&Path> = vec![
                &crush_dir,
                &local_state_dir,
                &crush_state_dir,
                &codex_dir,
                &aider_dir,
            ];
            if agent == "opencode" {
                base_dirs.push(&opencode_share);
            }
            for d in base_dirs {
                fs::create_dir_all(d).ok();
            }
        }

        {
            let mut pairs: Vec<(PathBuf, &str)> = vec![
                (crush_dir, "/home/coder/.local/share/crush"),
                (local_state_dir, "/home/coder/.local/state"),
                (crush_state_dir, "/home/coder/.crush"),
                (codex_dir, "/home/coder/.codex"),
                (aider_dir, "/home/coder/.aider"),
            ];
            if agent == "opencode" {
                pairs.push((opencode_share, "/home/coder/.local/share/opencode"));
            }
            for (src, dst) in pairs {
                volume_flags.push(OsString::from("-v"));
                volume_flags.push(path_pair(&src, dst));
            }
        }

        // OpenCode config/cache (HOME/XDG), OpenHands, Plandex
        #[cfg(windows)]
        let (opencode_config, opencode_cache) = {
            let cfg = env::var("APPDATA")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| host_home.join(".config"))
                .join("opencode");
            let lapp = env::var("LOCALAPPDATA")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| host_home.join(".cache"))
                .join("opencode");
            (cfg, lapp)
        };
        #[cfg(not(windows))]
        let (opencode_config, opencode_cache) = (
            host_home.join(".config").join("opencode"),
            host_home.join(".cache").join("opencode"),
        );

        let openhands_home = host_home.join(".openhands");
        let plandex_home = host_home.join(".plandex-home");

        {
            let mut extra_dirs: Vec<(PathBuf, &str)> = Vec::new();
            if agent == "opencode" {
                extra_dirs.push((opencode_config, "/home/coder/.config/opencode"));
                extra_dirs.push((opencode_cache, "/home/coder/.cache/opencode"));
            }
            if agent == "openhands" {
                extra_dirs.push((openhands_home, "/home/coder/.openhands"));
            }
            if agent == "plandex" {
                extra_dirs.push((plandex_home, "/home/coder/.plandex-home"));
            }
            for (src, dst) in extra_dirs {
                fs::create_dir_all(&src).ok();
                volume_flags.push(OsString::from("-v"));
                volume_flags.push(path_pair(&src, dst));
            }
        }
    }

    // Aider root-level config files: handled via config clone policy in entrypoint (Phase 1).
    // No direct bind-mount of original host files here.

    // Git config
    let gitconfig = host_home.join(".gitconfig");
    ensure_file_exists(&gitconfig).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(OsString::from(format!(
        "{}:/home/coder/.gitconfig:ro",
        gitconfig.display()
    )));

    // Timezone files (optional)
    for (host_path, container_path) in [
        ("/etc/localtime", "/etc/localtime"),
        ("/etc/timezone", "/etc/timezone"),
    ] {
        let hp = Path::new(host_path);
        if hp.exists() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!(
                "{}:{}:ro",
                hp.display(),
                container_path
            )));
        }
    }

    // Host logs dir
    let host_logs_dir = pwd.join("build").join("logs");
    fs::create_dir_all(&host_logs_dir).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&host_logs_dir, "/var/log/host"));

    // Claude desktop config: ensure host file exists and bind-mount it into the agent home.
    if let Some(host_claude_cfg) = host_claude_config_path(host_home) {
        let needs_create = !host_claude_cfg.exists();
        if needs_create {
            if let Some(parent) = host_claude_cfg.parent() {
                let _ = fs::create_dir_all(parent);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
                }
            }
            // Boilerplate content when the file does not exist yet.
            let content = r#"{"mcpServers": {}}"#;
            let _ = fs::write(&host_claude_cfg, content);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Treat as potentially sensitive; default to 0600.
                let _ = fs::set_permissions(&host_claude_cfg, fs::Permissions::from_mode(0o600));
            }
        }
        if host_claude_cfg.exists() {
            let container_claude_cfg = "/home/coder/.config/claude/claude_desktop_config.json";
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!(
                "{}:{}",
                host_claude_cfg.display(),
                container_claude_cfg
            )));
        }
    }

    // GnuPG (read-only host mount)
    let gnupg_dir = host_home.join(".gnupg");
    fs::create_dir_all(&gnupg_dir).ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&gnupg_dir, fs::Permissions::from_mode(0o700));
    }
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(OsString::from(format!(
        "{}:/home/coder/.gnupg-host:ro",
        gnupg_dir.display()
    )));

    // Phase 1: Coding agent config root (read-only host mount) for global or explicit configs.
    // Resolve host config dir: explicit env (AIFO_CONFIG_HOST_DIR or AIFO_CODER_CONFIG_HOST_DIR),
    // else ~/.config/aifo-coder, else ~/.aifo-coder. Mount policy:
    // - If an explicit env override is provided and points to an existing directory: always mount.
    // - If using auto-resolved defaults: mount only when the directory contains at least one file
    //   under "global/" or the agent-specific subdir (e.g., "aider/") to avoid empty mounts in pristine setups.
    let (cfg_host_dir, cfg_is_override) = {
        if let Ok(v) = env::var("AIFO_CONFIG_HOST_DIR") {
            let p = PathBuf::from(v.trim());
            if !p.as_os_str().is_empty() && p.is_dir() {
                (Some(p), true)
            } else {
                (None, false)
            }
        } else if let Ok(v) = env::var("AIFO_CODER_CONFIG_HOST_DIR") {
            let p = PathBuf::from(v.trim());
            if !p.as_os_str().is_empty() && p.is_dir() {
                (Some(p), true)
            } else {
                (None, false)
            }
        } else {
            let p1 = host_home.join(".config").join("aifo-coder");
            if p1.is_dir() {
                (Some(p1), false)
            } else {
                let p2 = host_home.join(".aifo-coder");
                if p2.is_dir() {
                    (Some(p2), false)
                } else {
                    (None, false)
                }
            }
        }
    };
    if let Some(cfg) = cfg_host_dir {
        let should_mount = if cfg_is_override {
            true
        } else {
            // Mount only when the host config dir contains at least one regular file
            // under either "global/" or the agent-specific subdir (e.g., "aider/").
            // Per-run staged dirs (aider-PID, codex-PID, etc.) are already mounted
            // explicitly above and do not influence this check.
            let mut any = false;
            for name in &["global", agent] {
                let d = cfg.join(name);
                if let Ok(rd) = fs::read_dir(&d) {
                    for ent in rd.flatten() {
                        if ent.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                            any = true;
                            break;
                        }
                    }
                }
                if any {
                    break;
                }
            }
            any
        };
        if should_mount {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&cfg, "/home/coder/.aifo-config-host:ro"));
        }
    } else {
        crate::warn_print(
            "coding agent host config dir not found; agents may use API env defaults. Set AIFO_CONFIG_HOST_DIR or create ~/.config/aifo-coder",
        );
    }

    // Optional shim dir
    if let Ok(shim_dir) = env::var("AIFO_SHIM_DIR") {
        if !shim_dir.trim().is_empty() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:/opt/aifo/bin:ro", shim_dir)));
        }
    }

    // Optional unix socket dir
    if let Ok(dir) = env::var("AIFO_TOOLEEXEC_UNIX_DIR") {
        if !dir.trim().is_empty() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:/run/aifo", dir)));
        }
    }

    volume_flags
}

fn collect_user_flags(uid_opt: Option<u32>, gid_opt: Option<u32>) -> Vec<OsString> {
    let mut user_flags: Vec<OsString> = Vec::new();
    if let (Some(uid), Some(gid)) = (uid_opt, gid_opt) {
        user_flags.push(OsString::from("--user"));
        user_flags.push(OsString::from(format!("{uid}:{gid}")));
    }
    user_flags
}

fn collect_security_flags(apparmor_profile: Option<&str>) -> Vec<OsString> {
    let mut security_flags: Vec<OsString> = Vec::new();
    if let Some(profile) = apparmor_profile {
        if crate::docker_supports_apparmor() {
            security_flags.push(OsString::from("--security-opt"));
            security_flags.push(OsString::from(format!("apparmor={profile}")));
        } else {
            crate::warn_print(
                "docker daemon does not report apparmor support. continuing without apparmor.",
            );
        }
    }
    security_flags
}

fn compute_container_identity(agent: &str, prefix: &str) -> (String, String) {
    let cn_env = env::var("AIFO_CODER_CONTAINER_NAME").ok();
    let cn_src = env::var("AIFO_CODER_CONTAINER_NAME_SOURCE").ok();
    let container_name = if let Some(ref v) = cn_env {
        if cn_src.as_deref() == Some("generated") && !v.contains(&format!("-{}-", agent)) {
            format!("{}-{}-{}", prefix, agent, crate::create_session_id())
        } else {
            v.clone()
        }
    } else {
        format!("{}-{}-{}", prefix, agent, crate::create_session_id())
    };
    let hostname = env::var("AIFO_CODER_HOSTNAME").unwrap_or_else(|_| container_name.clone());
    (container_name, hostname)
}

/// Helper: set/replace tag on an image reference (strip any digest, replace last tag after '/').
fn set_image_tag(image: &str, new_tag: &str) -> String {
    let base = image.split_once('@').map(|(n, _)| n).unwrap_or(image);
    let last_slash = base.rfind('/');
    let last_colon = base.rfind(':');
    let without_tag = match (last_slash, last_colon) {
        (Some(slash), Some(colon)) if colon > slash => &base[..colon],
        (None, Some(_colon)) => base.split(':').next().unwrap_or(base),
        _ => base,
    };
    format!("{}:{}", without_tag, new_tag)
}

/// Helper: apply agent image overrides from environment.
fn maybe_override_agent_image(image: &str) -> String {
    // Highest precedence: explicit full image override
    if let Ok(v) = env::var("AIFO_CODER_AGENT_IMAGE") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    // Next: per-agent tag override
    if let Ok(tag) = env::var("AIFO_CODER_AGENT_TAG") {
        let t = tag.trim();
        if !t.is_empty() {
            return set_image_tag(image, t);
        }
    }
    // Global tag override applies when no agent-specific override is set
    if let Ok(tag) = env::var("AIFO_TAG") {
        let t = tag.trim();
        if !t.is_empty() {
            return set_image_tag(image, t);
        }
    }
    image.to_string()
}

/// Derive registry host from an image reference (first component if qualified).
fn parse_registry_host(image: &str) -> Option<String> {
    if let Some((first, _rest)) = image.split_once('/') {
        if first.contains('.') || first.contains(':') || first == "localhost" {
            return Some(first.to_string());
        }
    }
    None
}

/// Check if an image exists locally via `docker image inspect`.
fn image_exists_locally(runtime: &Path, image: &str) -> bool {
    let status = Command::new(runtime)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok();
    status.map(|s| s.success()).unwrap_or(false)
}

/// Pull image and on auth failure interactively run `docker login` then retry once.
/// Verbose runs stream docker pull output; non-verbose prints a short notice before quiet pull.
fn pull_image_with_autologin(
    runtime: &Path,
    image: &str,
    verbose: bool,
    agent_label: Option<&str>,
) -> io::Result<()> {
    // Effective verbosity: honor explicit flag or env set by CLI --verbose.
    let eff_verbose = verbose || env::var("AIFO_CODER_VERBOSE").ok().as_deref() == Some("1");

    // Helper to do a pull with inherited stdio so progress is visible.
    let pull_inherit = |rt: &Path, img: &str| -> io::Result<bool> {
        let st = Command::new(rt).arg("pull").arg(img).status()?;
        Ok(st.success())
    };

    // Helper to do a pull with captured output so we can parse error text.
    let pull_captured = |rt: &Path, img: &str| -> io::Result<(bool, String)> {
        let out = Command::new(rt)
            .arg("pull")
            .arg(img)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        let ok = out.status.success();
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .to_ascii_lowercase();
        Ok((ok, combined))
    };

    let use_err = crate::color_enabled_stderr();

    if eff_verbose {
        crate::log_info_stderr(
            use_err,
            &format!("aifo-coder: docker: docker pull {}", image),
        );
        // First, try streaming pull with inherited stdio.
        if pull_inherit(runtime, image)? {
            return Ok(());
        }
        // If that failed, do a captured pull just to inspect the error text.
        let (_ok2, combined) = pull_captured(runtime, image)?;
        // Determine if this looks like an auth error.
        let auth_patterns = [
            "pull access denied",
            "permission denied",
            "authentication required",
            "unauthorized",
            "requested access to the resource is denied",
            "may require 'docker login'",
            "requires 'docker login'",
        ];
        let looks_auth_error = auth_patterns.iter().any(|p| combined.contains(p));
        let auto_enabled = env::var("AIFO_CODER_AUTO_LOGIN").ok().as_deref() != Some("0");
        let interactive = atty::is(atty::Stream::Stdin);

        if auto_enabled && interactive && looks_auth_error {
            // Try interactive docker login for this registry (if any), then retry pull (streaming).
            let host = parse_registry_host(image);
            let mut login_cmd = Command::new(runtime);
            login_cmd.arg("login");
            if let Some(h) = host.as_deref() {
                crate::log_info_stderr(use_err, &format!("aifo-coder: docker: docker login {}", h));
                login_cmd.arg(h);
            } else {
                crate::log_info_stderr(use_err, "aifo-coder: docker: docker login");
            }
            let st = login_cmd.status().map_err(|e| {
                io::Error::new(e.kind(), format!("docker login failed to start: {}", e))
            })?;
            if !st.success() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "docker login failed",
                ));
            }
            // Retry pull, streaming progress.
            if pull_inherit(runtime, image)? {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "docker pull failed after login",
            ));
        }

        // Fallback: try pulling unqualified tail repo:tag from Docker Hub when image is qualified
        {
            fn current_tag(img: &str) -> Option<String> {
                let base = img.split_once('@').map(|(n, _)| n).unwrap_or(img);
                let last_slash = base.rfind('/');
                let last_colon = base.rfind(':');
                match (last_slash, last_colon) {
                    (Some(s), Some(c)) if c > s => Some(base[c + 1..].to_string()),
                    (None, Some(c)) => Some(base[c + 1..].to_string()),
                    _ => None,
                }
            }
            if let Some(_host) = parse_registry_host(image) {
                let tag = current_tag(image).unwrap_or_else(|| "latest".to_string());
                let tail = image.rsplit('/').next().unwrap_or(image);
                let unqual = format!(
                    "{}:{}",
                    tail.split_once(':').map(|(n, _)| n).unwrap_or(tail),
                    tag
                );
                if eff_verbose {
                    crate::log_info_stderr(
                        use_err,
                        &format!("aifo-coder: docker: docker pull {}", unqual),
                    );
                }
                if pull_inherit(runtime, &unqual)? {
                    return Ok(());
                }
                return Err(io::Error::other(format!(
                    "docker pull failed; tried: {}, {}",
                    image, unqual
                )));
            }
        }
        // If we get here, all attempts in verbose mode failed; return a generic error.
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "docker pull failed",
        ))
    } else {
        // Non-verbose: print a short notice before quiet pull so users get feedback.
        let msg = if let Some(name) = agent_label {
            format!("aifo-coder: pulling agent image [{}]: {}", name, image)
        } else {
            format!("aifo-coder: pulling agent image: {}", image)
        };
        crate::log_info_stderr(use_err, &msg);

        let out = Command::new(runtime)
            .arg("pull")
            .arg(image)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        if out.status.success() {
            return Ok(());
        }

        let auto_enabled = env::var("AIFO_CODER_AUTO_LOGIN").ok().as_deref() != Some("0");
        let interactive = atty::is(atty::Stream::Stdin);
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
        .to_ascii_lowercase();
        let auth_patterns = [
            "pull access denied",
            "permission denied",
            "authentication required",
            "unauthorized",
            "requested access to the resource is denied",
            "may require 'docker login'",
            "requires 'docker login'",
        ];
        let looks_auth_error = auth_patterns.iter().any(|p| combined.contains(p));

        if auto_enabled && interactive && looks_auth_error {
            let host = parse_registry_host(image);
            // Short notice for login
            if let Some(h) = host.as_deref() {
                crate::log_info_stderr(use_err, &format!("aifo-coder: docker login {}", h));
            } else {
                crate::log_info_stderr(use_err, "aifo-coder: docker login");
            }
            // Run docker login (inherit stdio)
            let st = Command::new(runtime)
                .arg("login")
                .args(host.as_deref().map(|h| vec![h]).unwrap_or_default())
                .status()
                .map_err(|e| {
                    io::Error::new(e.kind(), format!("docker login failed to start: {}", e))
                })?;
            if !st.success() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "docker login failed",
                ));
            }
            // Retry pull quietly with short notice
            crate::log_info_stderr(use_err, "aifo-coder: retrying pull after login");
            let out2 = Command::new(runtime)
                .arg("pull")
                .arg(image)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()?;
            if out2.status.success() {
                return Ok(());
            }
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "docker pull failed after login",
            ));
        }

        {
            // Fallback: try pulling unqualified tail repo:tag from Docker Hub when image is qualified
            fn current_tag(img: &str) -> Option<String> {
                let base = img.split_once('@').map(|(n, _)| n).unwrap_or(img);
                let last_slash = base.rfind('/');
                let last_colon = base.rfind(':');
                match (last_slash, last_colon) {
                    (Some(s), Some(c)) if c > s => Some(base[c + 1..].to_string()),
                    (None, Some(c)) => Some(base[c + 1..].to_string()),
                    _ => None,
                }
            }
            if let Some(_host) = parse_registry_host(image) {
                let tag = current_tag(image).unwrap_or_else(|| "latest".to_string());
                let tail = image.rsplit('/').next().unwrap_or(image);
                let unqual = format!(
                    "{}:{}",
                    tail.split_once(':').map(|(n, _)| n).unwrap_or(tail),
                    tag
                );
                let out_hub = Command::new(runtime)
                    .arg("pull")
                    .arg(&unqual)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()?;
                if out_hub.status.success() {
                    Ok(())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        format!("docker pull failed; tried: {}, {}", image, unqual),
                    ))
                }
            } else {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!(
                        "docker pull failed (status {:?})",
                        out.status.code().unwrap_or(-1)
                    ),
                ))
            }
        }
    }
}

/// Compute the effective agent image for real run:
/// - Apply env overrides (AIFO_CODER_AGENT_IMAGE/TAG),
/// - Resolve registry/namespace,
/// - Prefer local "<name>:latest" when present.
pub fn compute_effective_agent_image_for_run(image: &str) -> io::Result<String> {
    // Allow tests to exercise tag logic without requiring Docker by honoring
    // AIFO_CODER_TEST_DISABLE_DOCKER: when set, skip local existence checks.
    let runtime = match container_runtime_path() {
        Ok(p) => Some(p),
        Err(e) => {
            if env::var("AIFO_CODER_TEST_DISABLE_DOCKER").ok().as_deref() == Some("1") {
                None
            } else {
                return Err(e);
            }
        }
    };

    // Apply env overrides (same as build path)
    let base_image = maybe_override_agent_image(image);

    // Tail repository name (drop any registry/namespace and tag)
    let tail_repo = {
        let base = base_image
            .split_once('@')
            .map(|(n, _)| n)
            .unwrap_or(base_image.as_str());
        let last = base.rsplit('/').next().unwrap_or(base);
        last.split_once(':')
            .map(|(n, _)| n)
            .unwrap_or(last)
            .to_string()
    };
    let rel_tag = format!("release-{}", env!("CARGO_PKG_VERSION"));
    let internal = crate::preferred_internal_registry_prefix_quiet();

    // Prefer local images in this order:
    // 1) unqualified :latest, 2) unqualified :release-<pkg>,
    // 3) internal-qualified :latest, 4) internal-qualified :release-<pkg>.
    if let Some(rt) = runtime.as_ref() {
        let candidates = [
            format!("{tail_repo}:latest"),
            format!("{tail_repo}:{rel_tag}"),
            if internal.is_empty() {
                String::new()
            } else {
                format!("{internal}{tail_repo}:latest")
            },
            if internal.is_empty() {
                String::new()
            } else {
                format!("{internal}{tail_repo}:{rel_tag}")
            },
        ];
        for c in candidates.iter().filter(|s| !s.is_empty()) {
            if image_exists_locally(rt.as_path(), c) {
                return Ok(c.to_string());
            }
        }
    }

    // Remote resolution: prefer internal registry via resolve_image (may qualify),
    // otherwise return unqualified (Docker Hub) reference.
    let resolved_image = crate::registry::resolve_image(&base_image);
    Ok(resolved_image)
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        skip(passthrough, image, apparmor_profile),
        fields(agent = %agent)
    )
)]
/// Build a docker run preview string without requiring docker in PATH (used for dry-run).
pub fn build_docker_preview_only(
    agent: &str,
    passthrough: &[String],
    image: &str,
    apparmor_profile: Option<&str>,
) -> String {
    // TTY flags
    let tty_flags: Vec<&str> = if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        vec!["-it"]
    } else {
        vec!["-i"]
    };

    let pwd = {
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    // UID/GID mapping (unix only; ignored elsewhere)
    #[cfg(unix)]
    let uid_opt = Some(u32::from(getuid()));
    #[cfg(unix)]
    let gid_opt = Some(u32::from(getgid()));
    #[cfg(not(unix))]
    let (uid_opt, gid_opt) = (None, None);

    // Env flags
    let env_flags = collect_env_flags(agent, uid_opt);

    // Volume mounts
    let host_home = home::home_dir().unwrap_or_else(|| PathBuf::from(""));
    let volume_flags = collect_volume_flags(agent, &host_home, &pwd);

    // User and security flags
    let user_flags = collect_user_flags(uid_opt, gid_opt);
    let security_flags = collect_security_flags(apparmor_profile);

    // Container identity
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let (container_name, hostname) = compute_container_identity(agent, &prefix);

    // Agent command and PATH value
    let (agent_abs, path_value) = agent_bin_and_path(agent);
    let mut agent_cmd = vec![agent_abs];
    agent_cmd.extend(passthrough.iter().cloned());
    let agent_joined = crate::shell_join(&agent_cmd);

    // Compose preview args
    let mut preview_args: Vec<String> = Vec::new();
    preview_args.push("docker".to_string());
    preview_args.push("run".to_string());
    preview_args.push("--rm".to_string());
    for f in tty_flags {
        preview_args.push(f.to_string());
    }
    preview_args.push("--name".to_string());
    preview_args.push(container_name.clone());
    preview_args.push("--hostname".to_string());
    preview_args.push(hostname.clone());

    // Session network (if provided)
    if let Ok(net) = env::var("AIFO_SESSION_NETWORK") {
        if !net.trim().is_empty() {
            preview_args.push("--network".to_string());
            preview_args.push(net);
        }
    }
    // Linux host add-host (if requested)
    #[cfg(target_os = "linux")]
    {
        if env::var("AIFO_TOOLEEXEC_ADD_HOST").ok().as_deref() == Some("1") {
            preview_args.push("--add-host".to_string());
            preview_args.push("host.docker.internal:host-gateway".to_string());
        }
    }

    for f in &volume_flags {
        preview_args.push(f.to_string_lossy().to_string());
    }
    let workspace_mount = format!("{}:/workspace", pwd.display());
    preview_args.push("-v".to_string());
    preview_args.push(workspace_mount);

    preview_args.push("-w".to_string());
    preview_args.push("/workspace".to_string());

    for f in &env_flags {
        preview_args.push(f.to_string_lossy().to_string());
    }
    for f in &user_flags {
        preview_args.push(f.to_string_lossy().to_string());
    }
    for f in &security_flags {
        preview_args.push(f.to_string_lossy().to_string());
    }

    let base_image = maybe_override_agent_image(image);
    let resolved_image = crate::registry::resolve_image(&base_image);
    preview_args.push(resolved_image.clone());
    preview_args.push("/bin/sh".to_string());
    preview_args.push("-c".to_string());

    let sh_cmd = format!(
        "set -e; umask 077; \
         if [ \"${{AIFO_AGENT_IGNORE_SIGINT:-0}}\" = \"1\" ]; then trap '' INT; fi; \
         export PATH=\"{path_value}\"; sed_port(){{ if [ \"${{AIFO_SED_PORTABLE:-1}}\" = \"1\" ]; then sed -i'' \"$@\"; else sed -i \"$@\"; fi; }}; \
         uid=\"$(id -u)\"; gid=\"$(id -g)\"; \
         mkdir -p \"$HOME\" \"$GNUPGHOME\"; chmod 700 \"$HOME\" \"$GNUPGHOME\" 2>/dev/null || true; chown \"$uid:$gid\" \"$HOME\" 2>/dev/null || true; \
         unset GPG_AGENT_INFO; gpgconf --kill gpg-agent >/dev/null 2>&1 || true; gpgconf --launch gpg-agent >/dev/null 2>&1 || true; \
         /usr/local/bin/aifo-entrypoint >/dev/null 2>&1 || true; \
         exec {agent_joined}"
    );
    preview_args.push(sh_cmd.clone());

    let mut parts = Vec::with_capacity(preview_args.len());
    for p in preview_args {
        parts.push(crate::shell_escape(&p));
    }
    parts.join(" ")
}

#[cfg_attr(
    feature = "otel",
    instrument(
        level = "info",
        skip(passthrough, image, apparmor_profile),
        fields(agent = %agent)
    )
)]
/// Build the docker run command for the given agent invocation, and return a preview string.
pub fn build_docker_cmd(
    agent: &str,
    passthrough: &[String],
    image: &str,
    apparmor_profile: Option<&str>,
) -> io::Result<(Command, String)> {
    let runtime = container_runtime_path()?;

    // TTY flags
    let tty_flags: Vec<&str> = if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        vec!["-it"]
    } else {
        vec!["-i"]
    };

    let pwd = {
        let p = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        fs::canonicalize(&p).unwrap_or(p)
    };

    // UID/GID mapping
    #[cfg(unix)]
    let uid_opt = Some(u32::from(getuid()));
    #[cfg(unix)]
    let gid_opt = Some(u32::from(getgid()));
    #[cfg(not(unix))]
    let (uid_opt, gid_opt) = (None, None);

    // Env flags
    let env_flags = collect_env_flags(agent, uid_opt);

    // Env flags collected via helper (collect_env_flags)

    // Volume mounts
    let host_home = home::home_dir().unwrap_or_else(|| PathBuf::from(""));
    let volume_flags = collect_volume_flags(agent, &host_home, &pwd);

    // User mapping
    let user_flags = collect_user_flags(uid_opt, gid_opt);

    // AppArmor security flags
    let security_flags = collect_security_flags(apparmor_profile);
    // Image prefix used for container naming
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());

    // Container name/hostname using helper
    let (container_name, hostname) = compute_container_identity(agent, &prefix);
    // Export only when we generated a fresh name, so tests don't see cross-agent reuse.
    let cn_env = env::var("AIFO_CODER_CONTAINER_NAME").ok();
    let cn_src = env::var("AIFO_CODER_CONTAINER_NAME_SOURCE").ok();
    if cn_env.is_none()
        || (cn_src.as_deref() == Some("generated")
            && !cn_env.as_ref().unwrap().contains(&format!("-{}-", agent)))
    {
        env::set_var("AIFO_CODER_CONTAINER_NAME", &container_name);
        env::set_var("AIFO_CODER_CONTAINER_NAME_SOURCE", "generated");
    }
    let name_flags = vec![
        OsString::from("--name"),
        OsString::from(&container_name),
        OsString::from("--hostname"),
        OsString::from(&hostname),
    ];

    // Agent command and PATH value
    let (agent_abs, path_value) = agent_bin_and_path(agent);
    let mut agent_cmd = vec![agent_abs];
    agent_cmd.extend(passthrough.iter().cloned());
    let agent_joined = crate::shell_join(&agent_cmd);

    // Record a docker "run" invocation metric for this agent.
    #[cfg(feature = "otel")]
    {
        crate::telemetry::metrics::record_docker_invocation("run");
        crate::telemetry::metrics::record_run(agent);
    }

    // Shell command inside container
    let sh_cmd = format!(
        "set -e; umask 077; \
         if [ \"${{AIFO_AGENT_IGNORE_SIGINT:-0}}\" = \"1\" ]; then trap '' INT; fi; \
         export PATH=\"{path_value}\"; sed_port(){{ if [ \"${{AIFO_SED_PORTABLE:-1}}\" = \"1\" ]; then sed -i'' \"$@\"; else sed -i \"$@\"; fi; }}; \
         uid=\"$(id -u)\"; gid=\"$(id -g)\"; \
         mkdir -p \"$HOME\" \"$GNUPGHOME\"; chmod 700 \"$HOME\" \"$GNUPGHOME\" 2>/dev/null || true; chown \"$uid:$gid\" \"$HOME\" 2>/dev/null || true; \
         if (command -v getent >/dev/null 2>&1 && ! getent passwd \"$uid\" >/dev/null 2>&1) || (! command -v getent >/dev/null 2>&1 && ! grep -q \"^[^:]*:[^:]*:$uid:\" /etc/passwd); then \
           mkdir -p \"$HOME/.nss_wrapper\"; \
           PASSWD_FILE=\"$HOME/.nss_wrapper/passwd\"; GROUP_FILE=\"$HOME/.nss_wrapper/group\"; \
           echo \"coder:x:${{uid}}:${{gid}}:,,,:$HOME:/bin/sh\" > \"$PASSWD_FILE\"; \
           echo \"coder:x:${{gid}}:\" > \"$GROUP_FILE\"; \
           for so in /usr/lib/*/libnss_wrapper.so /usr/lib/*/libnss_wrapper.so.* /usr/lib/libnss_wrapper.so /lib/*/libnss_wrapper.so /lib/*/libnss_wrapper.so.*; do if [ -f \"$so\" ]; then export LD_PRELOAD=\"${{LD_PRELOAD:+$LD_PRELOAD:}}$so\"; break; fi; done; \
           export NSS_WRAPPER_PASSWD=\"$PASSWD_FILE\" NSS_WRAPPER_GROUP=\"$GROUP_FILE\" USER=\"coder\" LOGNAME=\"coder\"; \
         fi; \
         if [ -n \"${{XDG_RUNTIME_DIR:-}}\" ]; then mkdir -p \"$XDG_RUNTIME_DIR/gnupg\" || true; chmod 700 \"$XDG_RUNTIME_DIR\" \"$XDG_RUNTIME_DIR/gnupg\" 2>/dev/null || true; fi; \
         mkdir -p \"$HOME/.aifo-logs\" || true; \
         if [ -t 0 ] || [ -t 1 ]; then export GPG_TTY=\"$(tty 2>/dev/null || echo /dev/tty)\"; fi; \
         touch \"$GNUPGHOME/gpg-agent.conf\"; sed_port -e \"/^pinentry-program /d\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; echo \"pinentry-program /usr/bin/pinentry-curses\" >> \"$GNUPGHOME/gpg-agent.conf\"; \
         sed_port -e \"/^log-file /d\" -e \"/^debug-level /d\" -e \"/^verbose$/d\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; \
         echo \"log-file /home/coder/.gnupg/gpg-agent.log\" >> \"$GNUPGHOME/gpg-agent.conf\"; echo \"debug-level basic\" >> \"$GNUPGHOME/gpg-agent.conf\"; echo \"verbose\" >> \"$GNUPGHOME/gpg-agent.conf\"; \
         if ! grep -q \"^allow-loopback-pinentry\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"allow-loopback-pinentry\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         if ! grep -q \"^default-cache-ttl \" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"default-cache-ttl 7200\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         if ! grep -q \"^max-cache-ttl \" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"max-cache-ttl 86400\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         for item in private-keys-v1.d openpgp-revocs.d pubring.kbx trustdb.gpg gpg.conf; do \
           if [ ! -e \"$GNUPGHOME/$item\" ] && [ -e \"/home/coder/.gnupg-host/$item\" ]; then \
             cp -a \"/home/coder/.gnupg-host/$item\" \"$GNUPGHOME/\" 2>/dev/null || true; \
           fi; \
         done; \
         touch \"$GNUPGHOME/gpg.conf\"; sed_port -e \"/^pinentry-mode /d\" \"$GNUPGHOME/gpg.conf\" 2>/dev/null || true; echo \"pinentry-mode loopback\" >> \"$GNUPGHOME/gpg.conf\"; \
         chmod -R go-rwx \"$GNUPGHOME\" 2>/dev/null || true; \
         unset GPG_AGENT_INFO; gpgconf --kill gpg-agent >/dev/null 2>&1 || true; \
         gpgconf --launch gpg-agent >/dev/null 2>&1 || true; \
         if [ -f \"/var/log/host/apparmor.log\" ]; then (nohup sh -c \"tail -n0 -F /var/log/host/apparmor.log >> \\\"$HOME/.aifo-logs/apparmor.log\\\" 2>&1\" >/dev/null 2>&1 &); fi; \
         /usr/local/bin/aifo-entrypoint >/dev/null 2>&1 || true; \
         exec {agent_joined}"
    );

    // docker run command
    let mut cmd = Command::new(&runtime);
    let mut preview_args: Vec<String> = Vec::new();

    // program
    preview_args.push("docker".to_string());

    // subcommand and common flags
    cmd.arg("run").arg("--rm");
    preview_args.push("run".to_string());
    preview_args.push("--rm".to_string());

    // TTY flags
    for f in tty_flags {
        cmd.arg(f);
        preview_args.push(f.to_string());
    }

    // name/hostname
    for f in name_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }
    // Phase 2: join the ephemeral session network if provided
    if let Ok(net) = env::var("AIFO_SESSION_NETWORK") {
        if !net.trim().is_empty() {
            cmd.arg("--network").arg(&net);
            preview_args.push("--network".to_string());
            preview_args.push(net);
        }
    }
    // Phase 2 (Linux): make host.docker.internal resolvable to host-gateway
    #[cfg(target_os = "linux")]
    {
        if env::var("AIFO_TOOLEEXEC_ADD_HOST").ok().as_deref() == Some("1") {
            cmd.arg("--add-host")
                .arg("host.docker.internal:host-gateway");
            preview_args.push("--add-host".to_string());
            preview_args.push("host.docker.internal:host-gateway".to_string());
        }
    }

    // volumes
    for f in &volume_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }
    let workspace_mount = format!("{}:/workspace", pwd.display());
    cmd.arg("-v").arg(&workspace_mount);
    preview_args.push("-v".to_string());
    preview_args.push(workspace_mount);

    // workdir
    cmd.arg("-w").arg("/workspace");
    preview_args.push("-w".to_string());
    preview_args.push("/workspace".to_string());

    // env flags
    for f in &env_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }

    // user flags
    for f in &user_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }

    // security flags
    for f in &security_flags {
        preview_args.push(f.to_string_lossy().to_string());
        cmd.arg(f);
    }

    // Use the image passed in exactly; do not rewrite an explicit CLI override here.
    // Defaults and local :latest preferences are handled upstream in main.rs when --image is not provided.
    let effective_image = image.to_string();
    // Pre-pull image and auto-login on permission denied (interactive)
    if !image_exists_locally(runtime.as_path(), &effective_image) {
        let _ = pull_image_with_autologin(runtime.as_path(), &effective_image, false, Some(agent));
    }

    cmd.arg(&effective_image);
    preview_args.push(effective_image.clone());

    // shell and command
    cmd.arg("/bin/sh").arg("-c").arg(&sh_cmd);
    preview_args.push("/bin/sh".to_string());
    preview_args.push("-c".to_string());
    preview_args.push(sh_cmd.clone());

    // Render preview string with conservative shell escaping
    let preview = {
        let mut parts = Vec::with_capacity(preview_args.len());
        for p in preview_args {
            parts.push(crate::shell_escape(&p));
        }
        parts.join(" ")
    };

    Ok((cmd, preview))
}

fn split_paths_env(v: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if v.is_empty() {
        return out;
    }
    // Use ':' as separator across platforms; paths are under ~/.config/aifo-coder so this is safe.
    for part in v.split(':') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            out.push(PathBuf::from(trimmed));
        }
    }
    out
}

/// Remove per-run staged config directories recorded in AIFO_CONFIG_STAGING_DIRS and
/// the legacy AIFO_AIDER_STAGING_DIR (best-effort).
pub fn cleanup_aider_staging_from_env() {
    // Legacy single-dir env (pre-multi-agent staging)
    if let Ok(p) = env::var("AIFO_AIDER_STAGING_DIR") {
        let path = PathBuf::from(p);
        let _ = fs::remove_dir_all(&path);
        std::env::remove_var("AIFO_AIDER_STAGING_DIR");
    }

    if let Ok(v) = env::var("AIFO_CONFIG_STAGING_DIRS") {
        for p in split_paths_env(&v) {
            let _ = fs::remove_dir_all(&p);
        }
        std::env::remove_var("AIFO_CONFIG_STAGING_DIRS");
    }
}
