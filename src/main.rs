use clap::{Parser, Subcommand};
use std::env;
use std::process::{Command, ExitCode};
use std::io;
use aifo_coder::{desired_apparmor_profile, preferred_registry_prefix, build_docker_cmd, acquire_lock};


fn print_startup_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("─────────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 🚀  Welcome to AIFO-Coder v{}  🚀 ", version);
    println!("─────────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 🔒 Secure by Design | 🌍 Cross-Platform | 🦀 Powered by Rust");
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
    println!("─────────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 📜 Copyright (c) 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of the Migros AI Foundation");
    println!("─────────────────────────────────────────────────────────────────────────────────────────────────");
    println!();
}

fn run_doctor(_verbose: bool) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!("aifo-coder doctor");
    eprintln!();
    eprintln!("  version: v{}", version);
    eprintln!("  host:    {} / {}", std::env::consts::OS, std::env::consts::ARCH);
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
    eprintln!("  virtualization: {}", virtualization);

    // Docker/AppArmor capabilities
    let apparmor_supported = aifo_coder::docker_supports_apparmor();
    eprintln!(
        "  docker AppArmor support: {}",
        if apparmor_supported { "yes" } else { "no" }
    );
    eprintln!();

    // Desired AppArmor profile
    let profile = desired_apparmor_profile();
    eprintln!(
        "  desired AppArmor profile: {}",
        profile.as_deref().unwrap_or("(disabled)")
    );
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
    eprintln!();

    // Helpful config/state locations (display with ~)
    let home = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("~"));
    let home_str = home.to_string_lossy().to_string();
    let mut show = |label: &str, path: std::path::PathBuf| {
        let pstr = path.display().to_string();
        let shown = if pstr.starts_with(&home_str) {
            format!("~{}", &pstr[home_str.len()..])
        } else {
            pstr
        };
        let exists = path.exists();
        let use_color = atty::is(atty::Stream::Stderr);
        let (icon, status) = if exists { ("✅", "found") } else { ("❌", "missing") };

        let colored_path = if use_color {
            if exists {
                format!("\x1b[32m{}\x1b[0m", shown) // green
            } else {
                format!("\x1b[31m{}\x1b[0m", shown) // red
            }
        } else {
            shown
        };

        let colored_status = if use_color {
            if exists {
                format!("\x1b[32m{}\x1b[0m", status) // green
            } else {
                format!("\x1b[31m{}\x1b[0m", status) // red
            }
        } else {
            status.to_string()
        };

        eprintln!("  {:14} {:<40} {} {}", label, colored_path, icon, colored_status);
    };

    // Aider files
    show("aider config:",   home.join(".aider.conf.yml"));
    show("aider metadata:", home.join(".aider.model.metadata.json"));
    show("aider settings:", home.join(".aider.model.settings.yml"));
    eprintln!();

    // Crush paths
    show("crush config:", home.join(".local").join("share").join("crush"));
    show("crush state:",  home.join(".crush"));
    eprintln!();

    // Codex path (requested as ~/codex)
    show("codex config:", home.join("codex"));
    eprintln!();

    eprintln!("doctor: completed diagnostics.");
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

    /// Prepare and print what would run, but do not execute
    #[arg(long)]
    dry_run: bool,

    #[command(subcommand)]
    command: Agent,
}

#[derive(Subcommand, Debug, Clone)]
enum Agent {
    /// Run diagnostics to check environment and configuration
    Doctor,
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

    // Doctor subcommand runs diagnostics without acquiring a lock
    if let Agent::Doctor = &cli.command {
        print_startup_banner();
        run_doctor(cli.verbose);
        return ExitCode::from(0);
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
        Agent::Doctor => unreachable!("Doctor subcommand is handled earlier and returns immediately"),
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
                eprintln!("aifo-coder: image: {image}");
                eprintln!("aifo-coder: agent: {agent}");
            }
            if cli.verbose || cli.dry_run {
                eprintln!("aifo-coder: docker: {preview}");
            }
            if cli.dry_run {
                eprintln!("aifo-coder: dry-run requested; not executing Docker.");
                drop(lock);
                return ExitCode::from(0);
            }
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
    let name_prefix = env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let tag = env::var("AIFO_CODER_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());
    let image_name = format!("{name_prefix}-{agent}:{tag}");
    let registry = preferred_registry_prefix();
    if registry.is_empty() {
        image_name
    } else {
        format!("{registry}{image_name}")
    }
}










