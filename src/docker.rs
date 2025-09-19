#![allow(clippy::module_name_repetitions)]
//! Docker command construction and runtime detection.

#[cfg(unix)]
use nix::unistd::{getgid, getuid};
use once_cell::sync::Lazy;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    if let Ok(p) = which("docker") {
        return Ok(p);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Docker is required but was not found in PATH.",
    ))
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
    let (uid, gid) = { (u32::from(getuid()), u32::from(getgid())) };

    // Forward selected env vars (inherit from host)
    let mut env_flags: Vec<OsString> = Vec::new();
    for var in PASS_ENV_VARS.iter().copied() {
        if let Ok(val) = env::var(var) {
            if !val.is_empty() {
                env_flags.push(OsString::from("-e"));
                env_flags.push(OsString::from(var));
            }
        }
    }

    // Always set these inside container
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("HOME=/home/coder"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("USER=coder"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("CODEX_HOME=/home/coder/.codex"));
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("GNUPGHOME=/home/coder/.gnupg"));
    // Ensure our auto-exit shell wrapper is preferred for transient shells
    env_flags.push(OsString::from("-e"));
    env_flags.push(OsString::from("SHELL=/opt/aifo/bin/sh"));

    // XDG_RUNTIME_DIR for gpg-agent sockets
    #[cfg(unix)]
    {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from(format!(
            "XDG_RUNTIME_DIR=/tmp/runtime-{uid}"
        )));
    }
    // Ensure pinentry can bind to the terminal when interactive sessions are used
    if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from("GPG_TTY=/dev/tty"));
    }

    // Map unified AIFO_* environment to agent-specific variables
    if let Ok(v) = env::var("AIFO_API_KEY") {
        if !v.is_empty() {
            // OpenAI-style
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_KEY={v}")));
            // Azure-style
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_API_KEY={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_KEY={v}")));
        }
    }
    if let Ok(v) = env::var("AIFO_API_BASE") {
        if !v.is_empty() {
            // OpenAI-style base URL
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_BASE_URL={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_BASE={v}")));
            // Azure-style endpoint/base
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_ENDPOINT={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_BASE={v}")));
            // Hint some clients that this is Azure-backed endpoint
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from("OPENAI_API_TYPE=azure"));
        }
    }
    if let Ok(v) = env::var("AIFO_API_VERSION") {
        if !v.is_empty() {
            // OpenAI-style API version (used by some clients for Azure)
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("OPENAI_API_VERSION={v}")));
            // Azure-style version
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_OPENAI_API_VERSION={v}")));
            env_flags.push(OsString::from("-e"));
            env_flags.push(OsString::from(format!("AZURE_API_VERSION={v}")));
        }
    }
    // Phase 2: pass through tool-exec proxy URL and token if set
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

    // Disable commit signing for Aider if requested
    if agent == "aider" {
        if let Ok(v) = env::var("AIFO_CODER_GIT_SIGN") {
            let vl = v.to_lowercase();
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

    // Volume mounts and host prep
    let mut volume_flags: Vec<OsString> = Vec::new();
    let host_home = home::home_dir().unwrap_or_else(|| PathBuf::from(""));

    // Per-pane state mounts (Phase 1): when AIFO_CODER_FORK_STATE_DIR is set, mount per-pane
    if let Ok(state_dir) = env::var("AIFO_CODER_FORK_STATE_DIR") {
        let sd = state_dir.trim();
        if !sd.is_empty() {
            let base = PathBuf::from(sd);
            let aider_dir = base.join(".aider");
            let codex_dir = base.join(".codex");
            let crush_dir = base.join(".crush");
            let _ = fs::create_dir_all(&aider_dir);
            let _ = fs::create_dir_all(&codex_dir);
            let _ = fs::create_dir_all(&crush_dir);
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&crush_dir, "/home/coder/.crush"));
        } else {
            // Fallback to legacy HOME-based mounts if the env var is empty
            let crush_dir = host_home.join(".local").join("share").join("crush");
            fs::create_dir_all(&crush_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&crush_dir, "/home/coder/.local/share/crush"));
            let crush_state_dir = host_home.join(".crush");
            fs::create_dir_all(&crush_state_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&crush_state_dir, "/home/coder/.crush"));
            let codex_dir = host_home.join(".codex");
            fs::create_dir_all(&codex_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));
            let aider_dir = host_home.join(".aider");
            fs::create_dir_all(&aider_dir).ok();
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));
        }
    } else {
        // Legacy HOME-based mounts (non-fork mode)
        let crush_dir = host_home.join(".local").join("share").join("crush");
        fs::create_dir_all(&crush_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&crush_dir, "/home/coder/.local/share/crush"));
        let crush_state_dir = host_home.join(".crush");
        fs::create_dir_all(&crush_state_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&crush_state_dir, "/home/coder/.crush"));
        let codex_dir = host_home.join(".codex");
        fs::create_dir_all(&codex_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));
        let aider_dir = host_home.join(".aider");
        fs::create_dir_all(&aider_dir).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));
    }

    // Aider root-level config files
    for fname in [
        ".aider.conf.yml",
        ".aider.model.metadata.json",
        ".aider.model.settings.yml",
    ] {
        let src = host_home.join(fname);
        ensure_file_exists(&src).ok();
        volume_flags.push(OsString::from("-v"));
        volume_flags.push(path_pair(&src, &format!("/home/coder/{fname}")));
    }

    // Git config
    let gitconfig = host_home.join(".gitconfig");
    ensure_file_exists(&gitconfig).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&gitconfig, "/home/coder/.gitconfig"));

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

    // GnuPG: mount host ~/.gnupg read-only to /home/coder/.gnupg-host
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
    // Optional: mount host-provided shim directory into PATH front inside the agent
    if let Ok(shim_dir) = env::var("AIFO_SHIM_DIR") {
        if !shim_dir.trim().is_empty() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:/opt/aifo/bin:ro", shim_dir)));
        }
    }

    // Phase 4 (Linux): mount unix socket directory if unix transport is enabled
    if let Ok(dir) = env::var("AIFO_TOOLEEXEC_UNIX_DIR") {
        if !dir.trim().is_empty() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:/run/aifo", dir)));
        }
    }

    // User mapping
    #[allow(unused_mut)]
    let mut user_flags: Vec<OsString> = Vec::new();
    #[cfg(unix)]
    {
        user_flags.push(OsString::from("--user"));
        user_flags.push(OsString::from(format!("{uid}:{gid}")));
    }

    // AppArmor security flags
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
    // Image prefix used for container naming
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());

    // Container name/hostname
    let container_name = env::var("AIFO_CODER_CONTAINER_NAME")
        .unwrap_or_else(|_| format!("{}-{}-{}", prefix, agent, crate::create_session_id()));
    // Make container name available to other subsystems (e.g., proxy) via env
    env::set_var("AIFO_CODER_CONTAINER_NAME", &container_name);
    let hostname = env::var("AIFO_CODER_HOSTNAME").unwrap_or_else(|_| container_name.clone());
    let name_flags = vec![
        OsString::from("--name"),
        OsString::from(&container_name),
        OsString::from("--hostname"),
        OsString::from(&hostname),
    ];

    // Agent command vector and join with shell escaping (use absolute agent paths; ensure system/venv precede shims)
    let agent_abs = match agent {
        "aider" => "/opt/venv/bin/aider",
        "codex" => "/usr/local/bin/codex",
        "crush" => "/usr/local/bin/crush",
        _ => agent,
    };
    let mut agent_cmd = vec![agent_abs.to_string()];
    agent_cmd.extend(passthrough.iter().cloned());
    let agent_joined = crate::shell_join(&agent_cmd);

    // Shell command inside container
    let sh_cmd = format!(
        "set -e; umask 077; \
         if [ \"${{AIFO_AGENT_IGNORE_SIGINT:-0}}\" = \"1\" ]; then trap '' INT; fi; \
         export PATH=\"/opt/venv/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:/opt/aifo/bin:$PATH\"; \
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
         touch \"$GNUPGHOME/gpg-agent.conf\"; sed -i \"/^pinentry-program /d\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; echo \"pinentry-program /usr/bin/pinentry-curses\" >> \"$GNUPGHOME/gpg-agent.conf\"; \
         sed -i \"/^log-file /d;/^debug-level /d;/^verbose$/d\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; \
         echo \"log-file /home/coder/.gnupg/gpg-agent.log\" >> \"$GNUPGHOME/gpg-agent.conf\"; echo \"debug-level basic\" >> \"$GNUPGHOME/gpg-agent.conf\"; echo \"verbose\" >> \"$GNUPGHOME/gpg-agent.conf\"; \
         if ! grep -q \"^allow-loopback-pinentry\" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"allow-loopback-pinentry\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         if ! grep -q \"^default-cache-ttl \" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"default-cache-ttl 7200\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         if ! grep -q \"^max-cache-ttl \" \"$GNUPGHOME/gpg-agent.conf\" 2>/dev/null; then echo \"max-cache-ttl 86400\" >> \"$GNUPGHOME/gpg-agent.conf\"; fi; \
         for item in private-keys-v1.d openpgp-revocs.d pubring.kbx trustdb.gpg gpg.conf; do \
           if [ ! -e \"$GNUPGHOME/$item\" ] && [ -e \"/home/coder/.gnupg-host/$item\" ]; then \
             cp -a \"/home/coder/.gnupg-host/$item\" \"$GNUPGHOME/\" 2>/dev/null || true; \
           fi; \
         done; \
         touch \"$GNUPGHOME/gpg.conf\"; sed -i \"/^pinentry-mode /d\" \"$GNUPGHOME/gpg.conf\" 2>/dev/null || true; echo \"pinentry-mode loopback\" >> \"$GNUPGHOME/gpg.conf\"; \
         chmod -R go-rwx \"$GNUPGHOME\" 2>/dev/null || true; \
         unset GPG_AGENT_INFO; gpgconf --kill gpg-agent >/dev/null 2>&1 || true; \
         gpgconf --launch gpg-agent >/dev/null 2>&1 || true; \
         if [ -f \"/var/log/host/apparmor.log\" ]; then (nohup sh -c \"tail -n0 -F /var/log/host/apparmor.log >> \\\"$HOME/.aifo-logs/apparmor.log\\\" 2>&1\" >/dev/null 2>&1 &); fi; \
         repo_name=\"$(git -C /workspace config --get user.name 2>/dev/null || true)\"; \
         repo_email=\"$(git -C /workspace config --get user.email 2>/dev/null || true)\"; \
         global_name=\"$(git config --global --get user.name 2>/dev/null || true)\"; \
         global_email=\"$(git config --global --get user.email 2>/dev/null || true)\"; \
         name=\"${{GIT_AUTHOR_NAME:-${{repo_name}}}}\"; [ -z \"$name\" ] || [ \"$name\" = \"Your Name\" ] && name=\"${{global_name:-$name}}\"; \
         email=\"${{GIT_AUTHOR_EMAIL:-${{repo_email}}}}\"; [ -z \"$email\" ] || [ \"$email\" = \"you@example.com\" ] && email=\"${{global_email:-$email}}\"; \
         if [ -n \"$name\" ]; then export GIT_AUTHOR_NAME=\"$name\" GIT_COMMITTER_NAME=\"$name\"; fi; \
         if [ -n \"$email\" ]; then export GIT_AUTHOR_EMAIL=\"$email\" GIT_COMMITTER_EMAIL=\"$email\"; fi; \
         case \"${{AIFO_CODER_GIT_SIGN:-}}\" in 0|false|FALSE|no|NO|off) want_sign=0 ;; *) want_sign=1 ;; esac; \
         if [ -d \"/workspace/.git\" ]; then \
           if [ \"$want_sign\" = \"1\" ]; then \
             git -C /workspace config --get commit.gpgsign >/dev/null 2>&1 || git -C /workspace config commit.gpgsign true; \
             git -C /workspace config --get gpg.program >/dev/null 2>&1 || git -C /workspace config gpg.program gpg; \
             if [ -n \"${{GIT_SIGNING_KEY:-}}\" ]; then git -C /workspace config user.signingkey \"$GIT_SIGNING_KEY\"; \
             else skey=\"$(gpg --list-secret-keys --with-colons 2>/dev/null | grep ^fpr: | head -n1 | cut -d: -f10)\"; [ -n \"$skey\" ] && git -C /workspace config user.signingkey \"$skey\" || true; fi; \
           else \
             git -C /workspace config commit.gpgsign false || true; \
           fi; \
         fi; \
         exec {agent_joined}"
    );

    // docker run command
    let mut cmd = Command::new(runtime);
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

    // image
    cmd.arg(image);
    preview_args.push(image.to_string());

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

// Local helper: Render a docker -v host:container pair.
fn path_pair(host: &Path, container: &str) -> OsString {
    OsString::from(format!("{}:{container}", host.display()))
}

// Local helper: Ensure a file exists by creating parent directories as needed.
fn ensure_file_exists(p: &Path) -> io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::File::create(p)?;
    }
    Ok(())
}
