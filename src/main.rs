use clap::{Parser, Subcommand};
use std::env;
use std::process::{Command, ExitCode};
use std::io;
use std::fs;
use std::path::PathBuf;
use aifo_coder::{desired_apparmor_profile, preferred_registry_prefix, build_docker_cmd, acquire_lock};


fn print_startup_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 🚀  Welcome to the Migros AI Foundaton Coder v{}  🚀 ", version);
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 🔒 Secure by Design | 🌍 Cross-Platform | 🦀 Powered by Rust | 🧠 Developed by AIFO");
    println!();
    println!(" ✨ Features:");
    println!("    - Linux: Coding agents run securely inside Docker containers with AppArmor.");
    println!("    - macOS: Transparent VM with Docker ensures isolated and secure agent execution.");
    println!();
    println!(" ⚙️  Starting up coding agents...");
    println!("    - Environment: [Secure Containerization Enabled]");
    println!("    - Platform: [Adaptive Security for Linux & macOS]");
    println!("    - Version: {}", version);
    println!();
    println!(" 🔧 Building a safer future for coding automation in Migros Group...");
    println!("    - Container isolation on Linux & macOS");
    println!("    - Agents run inside a container, not on your host runtimes");
    println!("    - No privileged Docker mode; no host Docker socket is mounted");
    println!("    - Minimal attack surface area");
    println!("    - Only the current project folder and essential per‑tool config/state paths are mounted");
    println!("    - Nothing else from your home directory is exposed by default");
    println!("    - Principle of least privilege");
    println!("    - AppArmor Support (via Docker)");
    println!("    - No additional host devices, sockets or secrets are mounted");
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 📜 Copyright (c) 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of Migros AI Foundation");
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!();
}

fn run_doctor(_verbose: bool) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!("aifo-coder doctor");
    eprintln!();
    let val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1mv{}\x1b[0m", version) } else { format!("v{}", version) };
    eprintln!("  version: {}", val);
    let host_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{} / {}\x1b[0m", std::env::consts::OS, std::env::consts::ARCH) } else { format!("{} / {}", std::env::consts::OS, std::env::consts::ARCH) };
    eprintln!("  host:    {}", host_val);
    eprintln!();

    // Virtualization environment
    let virtualization = if cfg!(target_os = "macos") {
        match Command::new("colima").arg("status").stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).output() {
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
    let virt_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", virtualization) } else { virtualization.to_string() };
    eprintln!("  virtualization: {}", virt_val);

    // Docker/AppArmor capabilities
    let apparmor_supported = aifo_coder::docker_supports_apparmor();
    let das = if apparmor_supported { "yes" } else { "no" };
    let das_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", das) } else { das.to_string() };
    eprintln!("  docker AppArmor support: {}", das_val);
    eprintln!();

    // Desired AppArmor profile
    let profile = aifo_coder::desired_apparmor_profile_quiet();
    let prof_str = profile.as_deref().unwrap_or("(disabled)");
    let prof_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", prof_str) } else { prof_str.to_string() };
    eprintln!("  docker AppArmor profile: {}", prof_val);

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
        args.push("cat /proc/self/attr/apparmor/current 2>/dev/null || echo unconfined".to_string());
        let mut cmd = Command::new("docker");
        for a in &args {
            cmd.arg(a);
        }
        let current = cmd.output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(|| "(unknown)".to_string());
        let current_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", current) } else { current };
        eprintln!("  apparmor in-container: {}", current_val);
    }
    eprintln!();

    // Docker command and version
    match aifo_coder::container_runtime_path() {
        Ok(p) => {
            let dc_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", p.display()) } else { format!("{}", p.display()) };
            eprintln!("  docker command:  {}", dc_val);
            if let Ok(out) = Command::new(&p).arg("--version").output() {
                let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
                // Typical: "Docker version 28.3.3, build 980b856816"
                let pretty = raw.trim_start_matches("Docker version ").to_string();
                let dv_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", pretty) } else { pretty };
                eprintln!("  docker version:  {}", dv_val);
            }
        }
        Err(_) => {
            eprintln!("  docker command:  (not found)");
        }
    }

    // Registry (quiet probe; no intermediate logs)
    let rp = aifo_coder::preferred_registry_prefix_quiet();
    let reg_display = if rp.is_empty() {
        "Docker Hub".to_string()
    } else {
        rp.trim_end_matches('/').to_string()
    };
    let reg_val = if atty::is(atty::Stream::Stderr) { format!("\x1b[34;1m{}\x1b[0m", reg_display) } else { reg_display };
    eprintln!("  docker registry: {}", reg_val);
    // (registry source suppressed)
    eprintln!();

    // Helpful config/state locations (display with ~)
    let home = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("~"));
    let home_str = home.to_string_lossy().to_string();
    let show = |label: &str, path: std::path::PathBuf, mounted: bool| {
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
        let path_col: usize = 44;    // target visible width for path column (moved left)
        let status_col: usize = 14;  // target width for each status cell (icon + text)

        // Compute visible width before building colored_path to avoid moving 'shown' prematurely.
        let visible_len = shown.chars().count();
        let pad_spaces = if visible_len < path_col { path_col - visible_len } else { 1 };
        let padding = " ".repeat(pad_spaces);

        // Colorize the path itself as a value (strong blue)
        let colored_path = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", shown) // strong blue
        } else {
            shown
        };

        // Build status cells (plain)
        let (icon1, text1) = if exists { ("✅", "found") } else { ("❌", "missing") };
        let (icon2, text2) = if mounted { ("✅", "mounted") } else { ("❌", "unmounted") };
        let cell1_plain = format!("{} {}", icon1, text1);
        let cell2_plain = format!("{} {}", icon2, text2);

        // Colorize statuses
        let colored_cell1 = if use_color {
            if exists {
                format!("\x1b[32m{}\x1b[0m", cell1_plain)
            } else {
                format!("\x1b[31m{}\x1b[0m", cell1_plain)
            }
        } else {
            cell1_plain.clone()
        };
        let colored_cell2 = if use_color {
            if mounted {
                format!("\x1b[32m{}\x1b[0m", cell2_plain)
            } else {
                format!("\x1b[31m{}\x1b[0m", cell2_plain)
            }
        } else {
            cell2_plain.clone()
        };

        // Pad the first status cell to a fixed width (based on plain text, not ANSI)
        let s1_visible_len = cell1_plain.chars().count();
        let s1_pad = if s1_visible_len < status_col { status_col - s1_visible_len } else { 1 };
        let s1_padding = " ".repeat(s1_pad);

        eprintln!(
            "  {:label_width$} {}{} {}{} {}",
            label,
            colored_path,
            padding,
            colored_cell1,
            s1_padding,
            colored_cell2,
            label_width = label_width
        );
    };

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
    let agent_ctx = std::env::var("AIFO_CODER_DOCTOR_AGENT").unwrap_or_else(|_| "aider".to_string());
    let mount_git = true;
    let mount_gnupg = true;
    let mount_aider = agent_ctx.eq_ignore_ascii_case("aider");
    let mount_crush = agent_ctx.eq_ignore_ascii_case("crush");
    let mount_codex = agent_ctx.eq_ignore_ascii_case("codex");

    show("git config:",   home.join(".gitconfig"), mount_git);
    show("gnupg config:", home.join(".gnupg"), mount_gnupg);
    eprintln!();

    // Aider files
    show("aider config:",   home.join(".aider.conf.yml"), mount_aider);
    show("aider metadata:", home.join(".aider.model.metadata.json"), mount_aider);
    show("aider settings:", home.join(".aider.model.settings.yml"), mount_aider);
    eprintln!();

    // Crush paths
    show("crush config:", home.join(".local").join("share").join("crush"), mount_crush);
    show("crush state:",  home.join(".crush"), mount_crush);
    eprintln!();

    // Codex path
    show("codex config:", home.join(".codex"), mount_codex);
    eprintln!();

    // Editor availability for installed images (full and/or slim) via crush image
    if aifo_coder::container_runtime_path().is_ok() {
        let prefix = std::env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
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
            if !present { continue; }

            if let Ok(out) = Command::new("docker")
                .args(["run", "--rm", "--entrypoint", "sh", &img, "-lc", check])
                .output()
            {
                let list = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let show = if list.is_empty() { "(none)".to_string() } else { list };
                let val = if use_color { format!("\x1b[34;1m{}\x1b[0m", show) } else { show };
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
                let show = if list.is_empty() { "(none)".to_string() } else { list };
                let val = if use_color { format!("\x1b[34;1m{}\x1b[0m", show) } else { show };
                eprintln!("  editors:        {}", val);
            }
        }
    }

    eprintln!();
    // Workspace write test to validate mounts and UID mapping
    if aifo_coder::container_runtime_path().is_ok() {
        let image = default_image_for_quiet("crush");
        let tmpname = format!(".aifo-coder-doctor-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        );
        let pwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let uid = Command::new("id").arg("-u").output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "0".to_string());
        let gid = Command::new("id").arg("-g").output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "0".to_string());

        let _ = Command::new("docker")
            .args([
                "run", "--rm",
                "--user", &format!("{uid}:{gid}"),
                "-v", &format!("{}:/workspace", pwd.display()),
                "-w", "/workspace",
                "-e", "HOME=/home/coder",
                "-e", "GNUPGHOME=/home/coder/.gnupg",
                &image,
                "sh", "-lc",
                &format!("echo ok > /workspace/{tmp} && id -u > /workspace/{tmp}.uid", tmp = tmpname),
            ])
            .status();

        let host_file = pwd.join(&tmpname);
        let host_uid_file = pwd.join(format!("{tmp}.uid", tmp = tmpname));
        if host_file.exists() && host_uid_file.exists() {
            // Present readiness line aligned with the first status column (found/missing)
            let use_color = atty::is(atty::Stream::Stderr);
            let label_width: usize = 16;
            let path_col: usize = 52;
            let yes_val = if use_color { "\x1b[34;1myes\x1b[0m".to_string() } else { "yes".to_string() };
            let status_plain = "✅ workspace ready".to_string();
            let status_colored = if use_color { format!("\x1b[32m{}\x1b[0m", status_plain) } else { status_plain };
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
            // Even if skipped/failed to create files, present a readiness line aligned with the first status column
            let use_color = atty::is(atty::Stream::Stderr);
            let label_width: usize = 16;
            let path_col: usize = 44;
            let yes_val = if use_color { "\x1b[34;1myes\x1b[0m".to_string() } else { "yes".to_string() };
            let status_plain = "✅ workspace ready".to_string();
            let status_colored = if use_color { format!("\x1b[32m{}\x1b[0m", status_plain) } else { status_plain };
            eprintln!(
                "  {:label_width$} {:<path_col$} {}",
                "workspace writable:",
                yes_val,
                status_colored,
                label_width = label_width,
                path_col = path_col
            );
        }
    }

    eprintln!();
    eprintln!("doctor: completed diagnostics.");
    eprintln!();
}

#[derive(Parser, Debug)]
#[command(name = "aifo-coder", version, about = "Run Codex, Crush or Aider inside Docker with current directory mounted.")]
struct Cli {
    /// Override Docker image (full ref). If unset, use per-agent default: {prefix}-{agent}:{tag}
    #[arg(long)]
    image: Option<String>,


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

    #[command(subcommand)]
    command: Agent,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, clap::ValueEnum)]
enum Flavor {
    Full,
    Slim,
}

#[derive(Subcommand, Debug, Clone)]
enum Agent {
    /// Run diagnostics to check environment and configuration
    Doctor,

    /// Show effective image references (including flavor/registry)
    Images,

    /// Clear on-disk caches (e.g., registry probe cache)
    CacheClear,

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
    let cli = Cli::parse();

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

    // Doctor subcommand runs diagnostics without acquiring a lock
    if let Agent::Doctor = &cli.command {
        print_startup_banner();
        run_doctor(cli.verbose);
        return ExitCode::from(0);
    } else if let Agent::Images = &cli.command {
        eprintln!("aifo-coder images");
        eprintln!();

        // Flavor and registry display
        let flavor_env = std::env::var("AIFO_CODER_IMAGE_FLAVOR").unwrap_or_default();
        let flavor = if flavor_env.trim().eq_ignore_ascii_case("slim") { "slim" } else { "full" };
        let rp = aifo_coder::preferred_registry_prefix_quiet();
        let reg_display = if rp.is_empty() { "Docker Hub".to_string() } else { rp.trim_end_matches('/').to_string() };

        let use_color = atty::is(atty::Stream::Stderr);
        let flavor_val = if use_color { format!("\x1b[34;1m{}\x1b[0m", flavor) } else { flavor.to_string() };
        let reg_val = if use_color { format!("\x1b[34;1m{}\x1b[0m", reg_display) } else { reg_display };

        eprintln!("  flavor:   {}", flavor_val);
        eprintln!("  registry: {}", reg_val);
        eprintln!();

        // Effective image references
        let codex_img = default_image_for("codex");
        let crush_img = default_image_for("crush");
        let aider_img = default_image_for("aider");
        let codex_val = if use_color { format!("\x1b[34;1m{}\x1b[0m", codex_img) } else { codex_img };
        let crush_val = if use_color { format!("\x1b[34;1m{}\x1b[0m", crush_img) } else { crush_img };
        let aider_val = if use_color { format!("\x1b[34;1m{}\x1b[0m", aider_img) } else { aider_img };
        eprintln!("  codex: {}", codex_val);
        eprintln!("  crush: {}", crush_val);
        eprintln!("  aider: {}", aider_val);
        eprintln!();

        return ExitCode::from(0);
    } else if let Agent::CacheClear = &cli.command {
        aifo_coder::invalidate_registry_cache();
        eprintln!("aifo-coder: cleared on-disk registry cache.");
        return ExitCode::from(0);
    }



    // Build docker command and run it
    let (agent, args) = match &cli.command {
        Agent::Codex { args } => ("codex", args.clone()),
        Agent::Crush { args } => ("crush", args.clone()),
        Agent::Aider { args } => ("aider", args.clone()),
        Agent::Doctor => unreachable!("Doctor subcommand is handled earlier and returns immediately"),
        Agent::Images => unreachable!("Images subcommand is handled earlier and returns immediately"),
        Agent::CacheClear => unreachable!("CacheClear subcommand is handled earlier and returns immediately"),
    };

    // Print startup banner before any further diagnostics
    print_startup_banner();

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
                    "aifo-coder: effective AppArmor profile: {}",
                    apparmor_profile.as_deref().unwrap_or("(disabled)")
                );
                // Show chosen registry and source for transparency
                let rp = aifo_coder::preferred_registry_prefix_quiet();
                let reg_display = if rp.is_empty() { "Docker Hub".to_string() } else { rp.trim_end_matches('/').to_string() };
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
            // Acquire lock only for real execution
            let lock = match acquire_lock() {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(1);
                }
            };
            let status = cmd.status().expect("failed to start docker");
            // Release lock before exiting
            drop(lock);
            ExitCode::from(status.code().unwrap_or(1) as u8)
        }
        Err(e) => {
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
    let name_prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
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
    let name_prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
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
