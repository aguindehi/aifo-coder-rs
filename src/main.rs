use clap::{Parser, Subcommand};
use once_cell::sync::Lazy;
use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use which::which;

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

#[derive(Parser, Debug)]
#[command(name = "aifo-coder", version, about = "Run Codex, Crush or Aider inside Docker with current directory mounted.")]
struct Cli {
    /// Override Docker image (full ref). If unset, use per-agent default: {prefix}-{agent}:{tag}
    #[arg(long)]
    image: Option<String>,

    /// Disable applying AppArmor profile even if configured via environment
    #[arg(long = "no-apparmor")]
    no_apparmor: bool,

    #[command(subcommand)]
    command: Agent,
}

#[derive(Subcommand, Debug, Clone)]
enum Agent {
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
}

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    if cli.no_apparmor {
        env::set_var("AIFO_CODER_NO_APPARMOR", "1");
    }

    // Acquire lock to prevent concurrent agent runs
    let lock = match acquire_lock() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };

    // Build docker command and run it
    let (agent, args) = match &cli.command {
        Agent::Codex { args } => ("codex", args.clone()),
        Agent::Crush { args } => ("crush", args.clone()),
        Agent::Aider { args } => ("aider", args.clone()),
    };

    let image = cli
        .image
        .clone()
        .unwrap_or_else(|| default_image_for(agent));

    match build_docker_cmd(agent, &args, &image) {
        Ok(mut cmd) => {
            let status = cmd.status().expect("failed to start docker");
            // Release lock before exiting
            drop(lock);
            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(e) => {
            drop(lock);
            eprintln!("{e}");
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
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    format!("{prefix}-{agent}:{tag}")
}

fn docker_path() -> io::Result<PathBuf> {
    which("docker").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "Docker is required but was not found in PATH."))
}

fn docker_supports_apparmor() -> bool {
    let docker = match docker_path() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let output = Command::new(docker)
        .args(["info", "--format", "{{json .SecurityOptions}}"])
        .output();
    let Ok(out) = output else { return false };
    let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
    s.contains("apparmor")
}

fn build_docker_cmd(agent: &str, passthrough: &[String], image: &str) -> io::Result<Command> {
    let docker = docker_path()?;

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
        use nix::unistd::{getgid, getuid};
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
    let no_aa = env::var("AIFO_CODER_NO_APPARMOR").ok().filter(|v| v == "1").is_some();
    let env_profile = env::var("AIFO_CODER_APPARMOR_PROFILE").ok();
    let use_aa = (!no_aa) && (env_profile.is_some() || env::var("AIFO_CODER_USE_APPARMOR").ok().as_deref() == Some("1"));
    let prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let _default_profile = "aifo-coder".to_string();
    let profile = env_profile.unwrap_or_else(|| if use_aa { _default_profile } else { String::new() });
    if !profile.is_empty() {
        if docker_supports_apparmor() {
            security_flags.push(OsString::from("--security-opt"));
            security_flags.push(OsString::from(format!("apparmor={profile}")));
        } else {
            eprintln!(
                "Warning: AppArmor profile '{}' requested but not supported by the Docker daemon. Continuing without AppArmor.",
                profile
            );
        }
    }

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
    let mut cmd = Command::new(docker);
    cmd.arg("run").arg("--rm");
    for f in tty_flags {
        cmd.arg(f);
    }
    for f in name_flags {
        cmd.arg(f);
    }
    for f in &volume_flags {
        cmd.arg(f);
    }
    cmd.arg("-v").arg(format!("{}:/workspace", pwd.display()));
    cmd.arg("-w").arg("/workspace");
    for f in &env_flags {
        cmd.arg(f);
    }
    for f in &user_flags {
        cmd.arg(f);
    }
    for f in &security_flags {
        cmd.arg(f);
    }
    cmd.arg(image);
    cmd.arg("/bin/sh").arg("-lc").arg(sh_cmd);

    Ok(cmd)
}

fn path_pair(host: &Path, container: &str) -> OsString {
    OsString::from(format!("{}:{container}", host.display()))
}

fn ensure_file_exists(p: &Path) -> io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        File::create(p)?;
    }
    Ok(())
}

fn shell_join(args: &[String]) -> String {
    args.iter().map(|a| shell_escape(a)).collect::<Vec<_>>().join(" ")
}

fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        "''".to_string()
    } else if s.chars().all(|c| c.is_ascii_alphanumeric() || "-_=./:@".contains(c)) {
        s.to_string()
    } else {
        let escaped = s.replace('\'', "'\"'\"'");
        format!("'{}'", escaped)
    }
}

fn acquire_lock() -> io::Result<File> {
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
                    let errno = nix::errno::errno();
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

fn candidate_lock_paths() -> Vec<PathBuf> {
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
