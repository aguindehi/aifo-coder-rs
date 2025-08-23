use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use which::which;
use once_cell::sync::Lazy;
use std::os::fd::AsRawFd;
use libc;
use atty;
#[cfg(unix)]
use nix::unistd::{getgid, getuid};

static PASS_ENV_VARS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        // OpenAI / Codex / generic
        "OPENAI_API_KEY",
        "OPENAI_ORG",
        "OPENAI_BASE_URL",
        "CODEX_OSS_BASE_URL",
        "CODEX_OSS_PORT",
        "CODEX_HOME",
        // Google / Vertex / Gemini
        "GEMINI_API_KEY",
        "VERTEXAI_PROJECT",
        "VERTEXAI_LOCATION",
        // Azure OpenAI (Crush) and Azure generic (Codex/Aider)
        "AZURE_OPENAI_API_ENDPOINT",
        "AZURE_OPENAI_API_KEY",
        "AZURE_OPENAI_API_VERSION",
        "AZURE_OPENAI_ENDPOINT",
        "AZURE_API_KEY",
        "AZURE_API_BASE",
        "AZURE_API_VERSION",
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
    ]
});

/// Locate the Docker runtime binary.
pub fn container_runtime_path() -> io::Result<PathBuf> {
    if let Ok(p) = which("docker") {
        return Ok(p);
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "Docker is required but was not found in PATH.",
    ))
}

/// Probe whether the Docker daemon reports AppArmor support, and (on Linux)
/// that the kernel AppArmor facility is enabled.
pub fn docker_supports_apparmor() -> bool {
    let runtime = match container_runtime_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let output = Command::new(runtime)
        .args(["info", "--format", "{{json .SecurityOptions}}"])
        .output();
    let Ok(out) = output else { return false };
    let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
    let docker_reports_apparmor = s.contains("apparmor");
    if !docker_reports_apparmor {
        return false;
    }
    // On Linux hosts, also require kernel AppArmor to be enabled.
    if cfg!(target_os = "linux") && !kernel_apparmor_enabled() {
        return false;
    }
    true
}

/// Best-effort detection of AppArmor being enabled in the Linux kernel.
/// Returns true if the kernel facility appears available/enabled.
fn kernel_apparmor_enabled() -> bool {
    // Prefer authoritative kernel knob when present
    if let Ok(content) = fs::read_to_string("/sys/module/apparmor/parameters/enabled") {
        let c = content.trim().to_lowercase();
        if c.starts_with('y') || c.contains("enforce") || c.contains("complain") || c == "1" || c == "yes" || c == "true" {
            // Double-check proc LSM interface presence
            return Path::new("/proc/self/attr/apparmor/current").exists()
                && Path::new("/proc/self/attr/apparmor/exec").exists();
        } else {
            return false;
        }
    }
    // Fallback: require both current and exec proc attributes to exist
    Path::new("/proc/self/attr/apparmor/current").exists()
        && Path::new("/proc/self/attr/apparmor/exec").exists()
}

#[cfg(target_os = "linux")]
fn apparmor_profile_available(name: &str) -> bool {
    if let Ok(list) = fs::read_to_string("/sys/kernel/security/apparmor/profiles") {
        for line in list.lines() {
            let l = line.trim();
            if l.is_empty() {
                continue;
            }
            if l.starts_with(&format!("{name} (")) || l.starts_with(&format!("{name} ")) {
                return true;
            }
        }
    }
    false
}

#[cfg(not(target_os = "linux"))]
fn apparmor_profile_available(_name: &str) -> bool {
    true
}

/// Choose the AppArmor profile to use, if any.
/// - If Docker supports AppArmor, prefer an explicit override via AIFO_CODER_APPARMOR_PROFILE.
/// - On macOS/Windows hosts (Docker-in-VM), default to docker-default to avoid requiring a host-installed custom profile.
/// - On native Linux hosts, prefer the custom "aifo-coder" profile if it is loaded; otherwise fall back to "docker-default"
///   if available; otherwise omit an explicit profile (Docker will choose its default).
pub fn desired_apparmor_profile() -> Option<String> {
    if !docker_supports_apparmor() {
        return None;
    }
    if let Ok(p) = env::var("AIFO_CODER_APPARMOR_PROFILE") {
        let trimmed = p.trim();
        let lower = trimmed.to_lowercase();
        // Allow explicit disabling via env var
        if trimmed.is_empty() || ["none", "no", "off", "false", "0", "disabled", "disable"].contains(&lower.as_str()) {
            return None;
        }
        if cfg!(target_os = "linux") && !apparmor_profile_available(trimmed) {
            eprintln!("aifo-coder: AppArmor profile '{}' not loaded on host; falling back to 'docker-default'.", trimmed);
            if apparmor_profile_available("docker-default") {
                return Some("docker-default".to_string());
            } else {
                eprintln!("aifo-coder: 'docker-default' profile not found; continuing without explicit AppArmor profile.");
                return None;
            }
        }
        return Some(trimmed.to_string());
    }
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        Some("docker-default".to_string())
    } else {
        if apparmor_profile_available("aifo-coder") {
            Some("aifo-coder".to_string())
        } else if apparmor_profile_available("docker-default") {
            eprintln!("aifo-coder: AppArmor profile 'aifo-coder' not loaded; using 'docker-default'.");
            Some("docker-default".to_string())
        } else {
            eprintln!("aifo-coder: No known AppArmor profile loaded; continuing without explicit profile.");
            None
        }
    }
}

fn is_host_port_reachable(host: &str, port: u16, timeout_ms: u64) -> bool {
    let addrs = (host, port).to_socket_addrs();
    if let Ok(addrs) = addrs {
        let timeout = Duration::from_millis(timeout_ms);
        for addr in addrs {
            if TcpStream::connect_timeout(&addr, timeout).is_ok() {
                return true;
            }
        }
    }
    false
}

/// Determine the preferred registry prefix for image references.
/// Precedence:
/// 1) If AIFO_CODER_REGISTRY_PREFIX is set:
///    - empty string forces Docker Hub (no prefix)
///    - non-empty is normalized to end with a single '/' and used as-is
/// 2) Otherwise, if repository.migros.net:443 is reachable, use "repository.migros.net/"
/// 3) Fallback: empty string (Docker Hub)
pub fn preferred_registry_prefix() -> String {
    if let Ok(pref) = env::var("AIFO_CODER_REGISTRY_PREFIX") {
        let trimmed = pref.trim();
        if trimmed.is_empty() {
            eprintln!("aifo-coder: AIFO_CODER_REGISTRY_PREFIX override set to empty; using Docker Hub (no registry prefix).");
            return String::new();
        }
        let mut s = trimmed.trim_end_matches('/').to_string();
        s.push('/');
        eprintln!("aifo-coder: Using AIFO_CODER_REGISTRY_PREFIX override: '{}'", s);
        return s;
    }

    // Prefer probing with curl for HTTPS reachability using short timeouts.
    if which("curl").is_ok() {
        eprintln!("aifo-coder: checking https://repository.migros.net/v2/ availability with: curl --connect-timeout 1 --max-time 2 -sSI ...");
        let status = Command::new("curl")
            .args([
                "--connect-timeout",
                "1",
                "--max-time",
                "2",
                "-sSI",
                "https://repository.migros.net/v2/",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Ok(st) = status {
            if st.success() {
                eprintln!("aifo-coder: repository.migros.net reachable; using registry prefix 'repository.migros.net/'.");
                return "repository.migros.net/".to_string();
            } else {
                eprintln!("aifo-coder: repository.migros.net not reachable (curl non-zero exit); using Docker Hub (no prefix).");
                return String::new();
            }
        } else {
            eprintln!("aifo-coder: curl invocation failed; falling back to TCP reachability check.");
        }
    } else {
        eprintln!("aifo-coder: curl not found; falling back to TCP reachability check.");
    }

    // Fallback quick TCP probe (short timeout).
    if is_host_port_reachable("repository.migros.net", 443, 300) {
        eprintln!("aifo-coder: repository.migros.net appears reachable via TCP; using registry prefix 'repository.migros.net/'.");
        "repository.migros.net/".to_string()
    } else {
        eprintln!("aifo-coder: repository.migros.net not reachable via TCP; using Docker Hub (no prefix).");
        String::new()
    }
}

/// Render a docker -v host:container pair.
pub fn path_pair(host: &Path, container: &str) -> OsString {
    OsString::from(format!("{}:{container}", host.display()))
}

/// Ensure a file exists by creating parent directories as needed.
pub fn ensure_file_exists(p: &Path) -> io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(p)?;
    }
    Ok(())
}

/// Join arguments with conservative shell escaping.
pub fn shell_join(args: &[String]) -> String {
    args.iter().map(|a| shell_escape(a)).collect::<Vec<_>>().join(" ")
}

/// Escape a single shell word safely for POSIX sh.
pub fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        "''".to_string()
    } else if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_=./:@".contains(c)) {
        s.to_string()
    } else {
        let escaped = s.replace('\'', "'\"'\"'");
        format!("'{}'", escaped)
    }
}

/// Candidate lock file locations, ordered by preference.
pub fn candidate_lock_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = home::home_dir() {
        paths.push(home.join(".aifo-coder.lock"));
    }
    if let Ok(rt) = env::var("XDG_RUNTIME_DIR") {
        if !rt.is_empty() {
            paths.push(PathBuf::from(rt).join("aifo-coder.lock"));
        }
    }
    paths.push(PathBuf::from("/tmp/aifo-coder.lock"));
    if let Ok(cwd) = env::current_dir() {
        paths.push(cwd.join(".aifo-coder.lock"));
    }
    paths
}

/// Build the docker run command for the given agent invocation, and return a preview string.
pub fn build_docker_cmd(agent: &str, passthrough: &[String], image: &str, apparmor_profile: Option<&str>) -> io::Result<(Command, String)> {
    let runtime = container_runtime_path()?;

    // TTY flags
    let tty_flags: Vec<&str> = if atty::is(atty::Stream::Stdin) || atty::is(atty::Stream::Stdout) {
        vec!["-it"]
    } else {
        vec!["-i"]
    };

    let pwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // UID/GID mapping
    #[cfg(unix)]
    let (uid, gid) = {
        (u32::from(getuid()), u32::from(getgid()))
    };

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

    // XDG_RUNTIME_DIR for gpg-agent sockets
    #[cfg(unix)]
    {
        env_flags.push(OsString::from("-e"));
        env_flags.push(OsString::from(format!("XDG_RUNTIME_DIR=/tmp/runtime-{uid}")));
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

    // Crush state
    let crush_dir = host_home.join(".local").join("share").join("crush");
    fs::create_dir_all(&crush_dir).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&crush_dir, "/home/coder/.local/share/crush"));

    // Codex state
    let codex_dir = host_home.join(".codex");
    fs::create_dir_all(&codex_dir).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&codex_dir, "/home/coder/.codex"));

    // Aider state dir
    let aider_dir = host_home.join(".aider");
    fs::create_dir_all(&aider_dir).ok();
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(path_pair(&aider_dir, "/home/coder/.aider"));

    // Aider root-level config files
    for fname in [".aider.conf.yml", ".aider.model.metadata.json", ".aider.model.settings.yml"] {
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
    for (host_path, container_path) in [("/etc/localtime", "/etc/localtime"), ("/etc/timezone", "/etc/timezone")] {
        let hp = Path::new(host_path);
        if hp.exists() {
            volume_flags.push(OsString::from("-v"));
            volume_flags.push(OsString::from(format!("{}:{}:ro", hp.display(), container_path)));
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
    // Best effort permission
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&gnupg_dir, fs::Permissions::from_mode(0o700));
    }
    volume_flags.push(OsString::from("-v"));
    volume_flags.push(OsString::from(format!("{}:/home/coder/.gnupg-host:ro", gnupg_dir.display())));

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
        if docker_supports_apparmor() {
            security_flags.push(OsString::from("--security-opt"));
            security_flags.push(OsString::from(format!("apparmor={profile}")));
        } else {
            eprintln!("Warning: Docker daemon does not report AppArmor support. Continuing without AppArmor.");
        }
    }
    // Image prefix used for container naming
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());

    // Container name/hostname
    let container_name = env::var("AIFO_CODER_CONTAINER_NAME")
        .unwrap_or_else(|_| format!("{}-{}", prefix, agent));
    let hostname = env::var("AIFO_CODER_HOSTNAME").unwrap_or_else(|_| container_name.clone());
    let name_flags = vec![OsString::from("--name"), OsString::from(&container_name), OsString::from("--hostname"), OsString::from(&hostname)];

    // Agent command vector and join with shell escaping
    let mut agent_cmd = vec![agent.to_string()];
    agent_cmd.extend(passthrough.iter().cloned());
    let agent_joined = shell_join(&agent_cmd);

    // Shell command inside container (copied from Python implementation)
    let sh_cmd = format!(
        "set -e; umask 077; \
         export PATH=\"/opt/venv/bin:$PATH\"; \
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
         for item in private-keys-v1.d openpgp-revocs.d pubring.kbx trustdb.gpg gpg.conf; do \
           if [ ! -e \"$GNUPGHOME/$item\" ] && [ -e \"/home/coder/.gnupg-host/$item\" ]; then \
             cp -a \"/home/coder/.gnupg-host/$item\" \"$GNUPGHOME/\" 2>/dev/null || true; \
           fi; \
         done; \
         touch \"$GNUPGHOME/gpg.conf\"; sed -i \"/^pinentry-mode /d\" \"$GNUPGHOME/gpg.conf\" 2>/dev/null || true; echo \"pinentry-mode loopback\" >> \"$GNUPGHOME/gpg.conf\"; \
         chmod -R go-rwx \"$GNUPGHOME\" 2>/dev/null || true; \
         gpgconf --kill gpg-agent >/dev/null 2>&1 || true; \
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
            parts.push(shell_escape(&p));
        }
        parts.join(" ")
    };

    Ok((cmd, preview))
}

/// Acquire a non-blocking exclusive lock using default candidate lock paths.
pub fn acquire_lock() -> io::Result<File> {
    let paths = candidate_lock_paths();
    let mut last_err: Option<io::Error> = None;

    for p in paths {
        // Best effort to ensure parent exists
        if let Some(parent) = p.parent() {
            let _ = fs::create_dir_all(parent);
        }
        match OpenOptions::new().create(true).read(true).write(true).open(&p) {
            Ok(f) => {
                // Try non-blocking exclusive lock
                let fd = f.as_raw_fd();
                let res = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
                if res == 0 {
                    return Ok(f);
                } else {
                    let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
                    if errno == libc::EWOULDBLOCK || errno == libc::EAGAIN {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Another coding agent is already running (lock held). Please try again later.",
                        ));
                    } else {
                        last_err = Some(io::Error::last_os_error());
                        continue;
                    }
                }
            }
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
    }

    let mut msg = String::from("Failed to create lock file in any candidate location: ");
    msg.push_str(
        &candidate_lock_paths()
            .into_iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    );
    if let Some(e) = last_err {
        msg.push_str(&format!(" (last error: {e})"));
    }
    Err(io::Error::new(io::ErrorKind::Other, msg))
}

/// Acquire a lock at a specific path (helper for tests).
pub fn acquire_lock_at(p: &Path) -> io::Result<File> {
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match OpenOptions::new().create(true).read(true).write(true).open(p) {
        Ok(f) => {
            let fd = f.as_raw_fd();
            let res = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if res == 0 {
                Ok(f)
            } else {
                let errno = io::Error::last_os_error().raw_os_error().unwrap_or(0);
                if errno == libc::EWOULDBLOCK || errno == libc::EAGAIN {
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Another coding agent is already running (lock held). Please try again later.",
                    ))
                } else {
                    Err(io::Error::last_os_error())
                }
            }
        }
        Err(e) => Err(e),
    }
}
