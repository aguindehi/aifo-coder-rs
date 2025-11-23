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

fn collect_volume_flags(agent: &str, host_home: &Path, pwd: &Path) -> Vec<OsString> {
    let mut volume_flags: Vec<OsString> = Vec::new();

    // Fork-state mounts or HOME-based mounts
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
        } else {
            // fallthrough to HOME-based below
        }
    }
    if volume_flags.is_empty() {
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

    // Aider root-level config files (only for aider agent)
    if agent == "aider" {
        for fname in [
            ".aider.conf.yml",
            ".aider.model.metadata.json",
            ".aider.model.settings.yml",
        ] {
            let src = host_home.join(fname);
            ensure_file_exists(&src).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!(
                "{}:/home/coder/{}:ro",
                src.display(),
                fname
            )));
        }
    }

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
    if let Ok(v) = env::var("AIFO_CODER_AGENT_IMAGE") {
        let t = v.trim();
        if !t.is_empty() {
            return t.to_string();
        }
    }
    if let Ok(tag) = env::var("AIFO_CODER_AGENT_TAG") {
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
fn pull_image_with_autologin(runtime: &Path, image: &str, verbose: bool) -> io::Result<()> {
    if verbose {
        let use_err = crate::color_enabled_stderr();
        crate::log_info_stderr(
            use_err,
            &format!("aifo-coder: docker: docker pull {}", image),
        );
    }
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
        let use_err = crate::color_enabled_stderr();
        // Run docker login interactively (inherit stdio)
        let mut login_cmd = Command::new(runtime);
        login_cmd.arg("login");
        if let Some(h) = host.as_deref() {
            if verbose {
                crate::log_info_stderr(use_err, &format!("aifo-coder: docker: docker login {}", h));
            }
            login_cmd.arg(h);
        } else if verbose {
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
        // Retry pull
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

    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!(
            "docker pull failed (status {:?})",
            out.status.code().unwrap_or(-1)
        ),
    ))
}

/// Derive "local latest" candidate for our agent images from a resolved ref.
/// E.g., "registry.intern.../aifo-coder-codex:release-0.6.3" -> "aifo-coder-codex:latest".
fn derive_local_latest_candidate(image: &str) -> Option<String> {
    // Strip digest
    let base = image.split_once('@').map(|(n, _)| n).unwrap_or(image);
    // Last path component: repository/name
    let last = base.rsplit('/').next().unwrap_or(base);
    // Strip tag (if present)
    let name_no_tag = match last.rfind(':') {
        Some(colon) => &last[..colon],
        None => last,
    };
    if name_no_tag.starts_with("aifo-coder-") {
        Some(format!("{}:latest", name_no_tag))
    } else {
        None
    }
}

/// Compute the effective agent image for real run:
/// - Apply env overrides (AIFO_CODER_AGENT_IMAGE/TAG),
/// - Resolve registry/namespace,
/// - Prefer local "<name>:latest" when present.
pub fn compute_effective_agent_image_for_run(image: &str) -> io::Result<String> {
    let runtime = container_runtime_path()?;
    // Apply env overrides (same as build path)
    let base_image = maybe_override_agent_image(image);
    let resolved_image = crate::registry::resolve_image(&base_image);

    // Helper: extract the current tag (suffix after ':' if present, not counting registry host:port)
    fn image_tag(img: &str) -> Option<&str> {
        let base = img.split_once('@').map(|(n, _)| n).unwrap_or(img);
        let last_slash = base.rfind('/');
        let last_colon = base.rfind(':');
        match (last_slash, last_colon) {
            (Some(slash), Some(colon)) if colon > slash => Some(&base[colon + 1..]),
            (None, Some(colon)) => Some(&base[colon + 1..]),
            _ => None,
        }
    }

    // Prefer local ":latest" only when we’re on the default tag (release-<pkg>) or already ":latest".
    let current_tag = image_tag(&resolved_image);
    let default_tag = format!("release-{}", env!("CARGO_PKG_VERSION"));
    let allow_local_latest = matches!(current_tag, Some(t) if t == "latest" || t == default_tag);

    if allow_local_latest {
        if let Some(candidate) = derive_local_latest_candidate(&resolved_image) {
            if image_exists_locally(runtime.as_path(), &candidate) {
                return Ok(candidate);
            }
        }
    }

    Ok(resolved_image)
}

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
    preview_args.push("-lc".to_string());

    let sh_cmd = format!(
        "set -e; umask 077; \
         if [ \"${{AIFO_AGENT_IGNORE_SIGINT:-0}}\" = \"1\" ]; then trap '' INT; fi; \
         export PATH=\"{path_value}\"; sed_port(){{ if [ \"${{AIFO_SED_PORTABLE:-1}}\" = \"1\" ]; then sed -i'' \"$@\"; else sed -i \"$@\"; fi; }}; \
         uid=\"$(id -u)\"; gid=\"$(id -g)\"; \
         mkdir -p \"$HOME\" \"$GNUPGHOME\"; chmod 700 \"$HOME\" \"$GNUPGHOME\" 2>/dev/null || true; chown \"$uid:$gid\" \"$HOME\" 2>/dev/null || true; \
         unset GPG_AGENT_INFO; gpgconf --kill gpg-agent >/dev/null 2>&1 || true; gpgconf --launch gpg-agent >/dev/null 2>&1 || true; \
         exec {agent_joined}"
    );
    preview_args.push(sh_cmd.clone());

    let mut parts = Vec::with_capacity(preview_args.len());
    for p in preview_args {
        parts.push(crate::shell_escape(&p));
    }
    parts.join(" ")
}

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

    // image: prefer local ":latest" when present, else resolved remote
    let effective_image = compute_effective_agent_image_for_run(image)?;
    // Pre-pull image and auto-login on permission denied (interactive)
    if !image_exists_locally(runtime.as_path(), &effective_image) {
        let _ = pull_image_with_autologin(runtime.as_path(), &effective_image, false);
    }

    cmd.arg(&effective_image);
    preview_args.push(effective_image.clone());

    // shell and command
    cmd.arg("/bin/sh").arg("-lc").arg(&sh_cmd);
    preview_args.push("/bin/sh".to_string());
    preview_args.push("-lc".to_string());
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
