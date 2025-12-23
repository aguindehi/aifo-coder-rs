pub(crate) fn print_startup_banner() {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!();
    eprintln!("──────────────────────────────────────────────────────────────────────────────────────────────");
    eprintln!(
        "      🚀  Welcome to the AI Foundation Coding Agent Wrapper  -  The AIFO Coder v{}   🚀 ",
        version
    );
    eprintln!("──────────────────────────────────────────────────────────────────────────────────────────────");
    eprintln!(" 🔒  Security by Design  |  🌍 Cross-Platform  |  🦀 Powered by Rust  |  🧠 Developed for you");
    eprintln!();

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
            let raw = String::from_utf8_lossy(&out.stdout).to_string();
            let parsed = aifo_coder::docker_security_options_parse(&raw);
            seccomp = parsed.seccomp_profile;
            cgroupns = parsed.cgroupns_mode;
            rootless = parsed.rootless;
        }
    }

    // Feature overview (Linux, macOS, Windows)
    eprintln!(" ✨ Features:");
    eprintln!(concat!(
        "    - Linux: Docker containers with AppArmor when available; seccomp ",
        "and cgroup namespaces."
    ));
    eprintln!(
        "    - macOS: Docker Desktop/Colima VM isolation; same security features inside the VM."
    );
    eprintln!(concat!(
        "    - Windows: Docker Desktop VM; Windows Terminal/PowerShell/",
        "Git Bash fork orchestration."
    ));
    eprintln!();

    // Dynamic startup summary (terse)
    eprintln!(" ⚙️  Starting up coding agents...");
    eprintln!(
        "    - Environment: Docker={} | Virt={}",
        docker_disp, virtualization
    );
    eprintln!("    - Platform: {}/{}", os, arch);
    let aa = if apparmor_supported {
        match apparmor_profile.as_deref() {
            Some(p) => format!("AppArmor=on ({})", p),
            None => "AppArmor=on".to_string(),
        }
    } else {
        "AppArmor=off".to_string()
    };
    eprintln!(
        "    - Security: {}, Seccomp={}, cgroupns={}, rootless={}",
        aa,
        seccomp,
        cgroupns,
        if rootless { "yes" } else { "no" }
    );
    eprintln!("    - Version: {}", version);
    eprintln!();

    // Safety highlights (concise, current capabilities)
    eprintln!(" 🔧 Building a safer future for coding automation...");
    eprintln!("    - Containerized agents; no privileged mode, no host Docker socket.");
    eprintln!(
        "    - AppArmor (Linux) with custom 'aifo-coder' or 'docker-default' when available."
    );
    eprintln!("    - Seccomp and cgroup namespaces as reported by Docker.");
    eprintln!("    - Per-pane isolated state for forks (.aider/.codex/.crush).");
    eprintln!(concat!(
        "    - Language toolchain sidecars (rust, node/ts, python, c/cpp, go) ",
        "via secure proxy."
    ));
    eprintln!("    - Optional unix:// proxy on Linux; host-gateway bridging when needed.");
    eprintln!("    - Minimal mounts: project workspace, config files, optional GnuPG keyrings.");
    eprintln!("──────────────────────────────────────────────────────────────────────────────────────────────");
    eprintln!("                 📜 Written 2025 by Amir Guindehi <amir@guindehi.ch>");
    eprintln!("──────────────────────────────────────────────────────────────────────────────────────────────");
    eprintln!();
}
