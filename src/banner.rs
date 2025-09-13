pub(crate) fn print_startup_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!();
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(
        " 🚀  Welcome to the Migros AI Foundation Coder - AIFO Coder v{}                     🚀 ",
        version
    );
    println!("─────────────────────────────────────────────────────────────────────────────────────────────");
    println!(" 🔒 Secure by Design  |  🌍 Cross-Platform  |  🦀 Powered by Rust  |  🧠 Developed by AIFO");
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
