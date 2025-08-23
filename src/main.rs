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
    eprintln!("  version: v{}", version);
    eprintln!("  host: {} / {}", std::env::consts::OS, std::env::consts::ARCH);

    match aifo_coder::container_runtime_path() {
        Ok(p) => {
            eprintln!("  docker: {}", p.display());
            if let Ok(out) = Command::new(&p).arg("--version").output() {
                let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !s.is_empty() {
                    eprintln!("  docker --version: {}", s);
                }
            }
        }
        Err(e) => {
            eprintln!("  docker: not found ({e})");
        }
    }

    let apparmor_supported = aifo_coder::docker_supports_apparmor();
    eprintln!(
        "  docker AppArmor support: {}",
        if apparmor_supported { "yes" } else { "no" }
    );

    let profile = desired_apparmor_profile();
    eprintln!(
        "  desired AppArmor profile: {}",
        profile.as_deref().unwrap_or("(disabled)")
    );

    let reg = preferred_registry_prefix();
    if reg.is_empty() {
        eprintln!("  registry: Docker Hub (no prefix)");
    } else {
        eprintln!("  registry: {}", reg);
    }

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










