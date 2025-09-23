#![allow(clippy::module_name_repetitions)]
//! Diagnostics (doctor) output.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::agent_images::default_image_for_quiet;

pub fn run_doctor(verbose: bool) {
    let verbose_env = std::env::var("AIFO_CODER_DOCTOR_VERBOSE")
        .ok()
        .map(|v| {
            let vl = v.to_lowercase();
            matches!(vl.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false);
    let verbose = verbose || verbose_env;

    let version = env!("CARGO_PKG_VERSION");
    eprintln!("aifo-coder doctor");
    eprintln!();
    eprintln!("  version: v{}", version);
    eprintln!(
        "  host:    {} / {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    eprintln!();

    // Virtualization environment
    let virtualization = if cfg!(target_os = "macos") {
        match Command::new("colima")
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
                    "Docker Desktop or other"
                }
            }
            Err(_) => "Docker Desktop or other",
        }
    } else {
        "native"
    };
    eprintln!("  virtualization: {}", virtualization);
    eprintln!();

    // Docker/AppArmor capabilities
    let apparmor_supported = aifo_coder::docker_supports_apparmor();
    let das = if apparmor_supported { "yes" } else { "no" };
    let das_val = if atty::is(atty::Stream::Stderr) {
        format!("\x1b[34;1m{}\x1b[0m", das)
    } else {
        das.to_string()
    };
    eprintln!("  docker apparmor support: {}", das_val);

    // Parse and display Docker security options (from `docker info`)
    if let Ok(rt) = aifo_coder::container_runtime_path() {
        if let Ok(out) = Command::new(&rt)
            .args(["info", "--format", "{{json .SecurityOptions}}"])
            .output()
        {
            let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
            // Extract JSON string array items without external deps
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
            let pretty: Vec<String> = items
                .iter()
                .cloned()
                .map(|s| {
                    let mut name: Option<String> = None;
                    let mut attrs: Vec<String> = Vec::new();
                    for part in s.split(',') {
                        if let Some(v) = part.strip_prefix("name=") {
                            name = Some(v.to_string());
                        } else {
                            attrs.push(part.to_string());
                        }
                    }
                    match name {
                        Some(n) => {
                            if attrs.is_empty() {
                                n
                            } else {
                                format!("{} ({})", n, attrs.join(", "))
                            }
                        }
                        None => s,
                    }
                })
                .collect();
            let joined = if pretty.is_empty() {
                "(none)".to_string()
            } else {
                pretty.join(", ")
            };
            let joined_val = joined.clone();
            eprintln!("  docker security options: {}", joined_val);
            {
                let has_apparmor = items.iter().any(|s| s.contains("apparmor"));
                // Extract seccomp profile if present
                let mut seccomp = String::from("(unknown)");
                for s in &items {
                    if s.contains("name=seccomp") {
                        for part in s.split(',') {
                            if let Some(v) = part.strip_prefix("profile=") {
                                seccomp = v.to_string();
                                break;
                            }
                        }
                        break;
                    }
                }
                // Extract cgroupns mode if present
                let mut cgroupns = String::from("(unknown)");
                for s in &items {
                    if s.contains("name=cgroupns") {
                        for part in s.split(',') {
                            if let Some(v) = part.strip_prefix("mode=") {
                                cgroupns = v.to_string();
                                break;
                            }
                        }
                        break;
                    }
                }
                let rootless = items.iter().any(|s| s.contains("rootless"));
                eprintln!(
                    "  docker security details: AppArmor={}, Seccomp={}, cgroupns={}, rootless={}",
                    if has_apparmor { "yes" } else { "no" },
                    seccomp,
                    cgroupns,
                    if rootless { "yes" } else { "no" }
                );
            }
            if verbose {
                let has_apparmor = items.iter().any(|s| s.contains("apparmor"));
                // Extract seccomp profile if present
                let mut seccomp = String::from("(unknown)");
                for s in &items {
                    if s.contains("name=seccomp") {
                        for part in s.split(',') {
                            if let Some(v) = part.strip_prefix("profile=") {
                                seccomp = v.to_string();
                                break;
                            }
                        }
                        break;
                    }
                }

                // security details were printed above in non-verbose section; only show tips here
                if !has_apparmor {
                    eprintln!("    tip: AppArmor not reported by Docker. On Linux, enable the AppArmor kernel module and ensure Docker is built with AppArmor support.");
                }
                if seccomp.eq_ignore_ascii_case("unconfined") {
                    eprintln!("    tip: Docker daemon seccomp profile is 'unconfined'. Consider switching to the default seccomp profile for better isolation.");
                }
            }
        }
    }
    eprintln!();

    // Desired AppArmor profile
    let profile = aifo_coder::desired_apparmor_profile_quiet();
    let prof_str = profile.as_deref().unwrap_or("(disabled)");
    eprintln!("  apparmor profile:      {}", prof_str);

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
        args.push(
            "cat /proc/self/attr/apparmor/current 2>/dev/null || echo unconfined".to_string(),
        );
        let mut cmd = Command::new("docker");
        for a in &args {
            cmd.arg(a);
        }
        let current = cmd
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "(unknown)".to_string());
        let current_trim = current.trim().to_string();
        eprintln!("  apparmor in-container: {}", current_trim);

        // Validate AppArmor status against expectations
        let expected = profile.as_deref();
        let expected_disp = expected.unwrap_or("(none)");

        let status_plain = {
            if !apparmor_supported {
                "skipped".to_string()
            } else if current_trim == "(unknown)" || current_trim.is_empty() {
                "unknown".to_string()
            } else if current_trim == "unconfined" {
                "FAIL".to_string()
            } else if let Some(p) = expected {
                if current_trim.starts_with(p) {
                    "PASS".to_string()
                } else {
                    "WARN".to_string()
                }
            } else {
                "PASS".to_string()
            }
        };
        eprintln!(
            "  apparmor validation:   {} (expected: {})",
            status_plain, expected_disp
        );
        if verbose {
            match status_plain.as_str() {
                "FAIL" => {
                    if cfg!(target_os = "linux") {
                        eprintln!(
                            "    tip: Container is unconfined. Generate and load the profile:"
                        );
                        eprintln!("    tip:   make apparmor");
                        eprintln!(
                            "    tip:   sudo apparmor_parser -r -W \"build/apparmor/aifo-coder\""
                        );
                        eprintln!("    tip: Then re-run with AppArmor enabled.");
                    } else {
                        eprintln!("    tip: Container appears unconfined. Ensure your Docker VM/distribution supports AppArmor and it is enabled.");
                    }
                }
                "WARN" => {
                    eprintln!("    tip: Active AppArmor profile differs from expected. If you set AIFO_CODER_APPARMOR_PROFILE, verify the profile is loaded on the host ('/sys/kernel/security/apparmor/profiles').");
                }
                "unknown" => {
                    eprintln!("    tip: Unable to read AppArmor status from container. Ensure 'docker run' works and that /proc/self/attr/apparmor/current is accessible.");
                }
                _ => {}
            }
        }
    }
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
            if verbose {
                eprintln!("    tip: Install Docker and ensure 'docker' is in your PATH. On Linux, install Docker Engine; on macOS, install Docker Desktop or use Colima.");
            }
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
    // (registry source suppressed)
    eprintln!();

    // Print stale fork sessions notice during doctor runs (Phase 6)
    aifo_coder::fork_print_stale_notice();
    eprintln!();

    // Helpful config/state locations (display with ~)
    let home = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("~"));
    let home_str = home.to_string_lossy().to_string();
    let show = |label: &str, path: std::path::PathBuf, _mounted: bool| {
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
        let path_col: usize = 44; // target visible width for path column (moved left)
        let _status_col: usize = 14; // deprecated: second status column removed

        // Compute visible width before building colored_path to avoid moving 'shown' prematurely.
        let visible_len = shown.chars().count();
        let pad_spaces = if visible_len < path_col {
            path_col - visible_len
        } else {
            1
        };
        let padding = " ".repeat(pad_spaces);

        // Colorize the path itself as a value (strong blue)
        let colored_path = if use_color {
            format!("\x1b[34;1m{}\x1b[0m", shown) // strong blue
        } else {
            shown
        };

        // Build status cells (plain)
        let (icon1, text1) = if exists {
            ("✅", "found")
        } else {
            ("❌", "missing")
        };
        let cell1_plain = format!("{} {}", icon1, text1);

        // Colorize status
        let colored_cell1 = if use_color {
            if exists {
                format!("\x1b[32m{}\x1b[0m", cell1_plain)
            } else {
                format!("\x1b[31m{}\x1b[0m", cell1_plain)
            }
        } else {
            cell1_plain.clone()
        };

        eprintln!(
            "  {:label_width$} {}{} {}",
            label,
            colored_path,
            padding,
            colored_cell1,
            label_width = label_width
        );
    };

    // Editor availability for installed images (full and/or slim) via crush image
    if aifo_coder::container_runtime_path().is_ok() {
        let prefix =
            std::env::var("AIFO_CODER_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
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
            if !present {
                continue;
            }

            if let Ok(out) = Command::new("docker")
                .args(["run", "--rm", "--entrypoint", "sh", &img, "-lc", check])
                .output()
            {
                let list = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let show = if list.is_empty() {
                    "(none)".to_string()
                } else {
                    list
                };
                let val = if use_color {
                    format!("\x1b[34;1m{}\x1b[0m", show)
                } else {
                    show
                };
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
                let show = if list.is_empty() {
                    "(none)".to_string()
                } else {
                    list
                };
                let val = if use_color {
                    format!("\x1b[34;1m{}\x1b[0m", show)
                } else {
                    show
                };
                eprintln!("  editors:        {}", val);
            }
        }
    }
    eprintln!();

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
    let agent_ctx =
        std::env::var("AIFO_CODER_DOCTOR_AGENT").unwrap_or_else(|_| "aider".to_string());
    let mount_git = true;
    let mount_gnupg = true;
    let mount_aider = agent_ctx.eq_ignore_ascii_case("aider");
    let mount_crush = agent_ctx.eq_ignore_ascii_case("crush");
    let mount_codex = agent_ctx.eq_ignore_ascii_case("codex");

    show("git config:", home.join(".gitconfig"), mount_git);
    show("gnupg config:", home.join(".gnupg"), mount_gnupg);
    eprintln!();

    // Git configuration (repo-first precedence) and signing diagnostics
    let git_available = Command::new("git")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if git_available {
        // Detect if we are inside a git repository and determine repo root
        let in_repo = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .eq_ignore_ascii_case("true")
            })
            .unwrap_or(false);

        let repo_root: Option<PathBuf> = if in_repo {
            Command::new("git")
                .args(["rev-parse", "--show-toplevel"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .ok()
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(s))
                    }
                })
        } else {
            None
        };

        // Helpers to read git configuration
        let git_get_repo = |key: &str| -> Option<String> {
            repo_root.as_ref().and_then(|root| {
                Command::new("git")
                    .arg("-C")
                    .arg(root)
                    .args(["config", "--get", key])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .output()
                    .ok()
                    .and_then(|o| {
                        let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        if v.is_empty() {
                            None
                        } else {
                            Some(v)
                        }
                    })
            })
        };
        let git_get_global = |key: &str| -> Option<String> {
            Command::new("git")
                .args(["config", "--global", "--get", key])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .ok()
                .and_then(|o| {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if v.is_empty() {
                        None
                    } else {
                        Some(v)
                    }
                })
        };

        // Gather identity values
        let repo_name = git_get_repo("user.name");
        let repo_email = git_get_repo("user.email");
        let global_name = git_get_global("user.name");
        let global_email = git_get_global("user.email");
        let env_name = std::env::var("GIT_AUTHOR_NAME")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let env_email = std::env::var("GIT_AUTHOR_EMAIL")
            .ok()
            .filter(|v| !v.trim().is_empty());

        let effective_name = env_name
            .clone()
            .or(repo_name.clone())
            .or(global_name.clone());
        let effective_email = env_email
            .clone()
            .or(repo_email.clone())
            .or(global_email.clone());

        // Validation helpers
        let looks_name_ok = |v: &str| !v.trim().is_empty() && v.trim() != "Your Name";
        let looks_email_ok = |v: &str| {
            let t = v.trim();
            !t.is_empty() && t != "you@example.com" && t.contains('@')
        };

        // Formatting helpers
        let use_color = atty::is(atty::Stream::Stderr);
        let blue = |s: &str| {
            if use_color {
                format!("\x1b[34;1m{}\x1b[0m", s)
            } else {
                s.to_string()
            }
        };
        let ok_cell = |set: bool| {
            if use_color {
                if set {
                    "\x1b[32m✅ set\x1b[0m".to_string()
                } else {
                    "\x1b[31m❌ unset\x1b[0m".to_string()
                }
            } else if set {
                "✅ set".to_string()
            } else {
                "❌ unset".to_string()
            }
        };

        let env_cell = |present: bool, needed: bool| {
            if present {
                if use_color {
                    "\x1b[32m✅ found\x1b[0m".to_string()
                } else {
                    "✅ found".to_string()
                }
            } else if !needed {
                if use_color {
                    "\x1b[34;1m➖ not needed\x1b[0m".to_string()
                } else {
                    "➖ not needed".to_string()
                }
            } else if use_color {
                "\x1b[31m❌ missing\x1b[0m".to_string()
            } else {
                "❌ missing".to_string()
            }
        };

        let label_w: usize = 16;
        let name_w: usize = 44;

        // Identity rows: name
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "git identity:",
            "repo user.name",
            ok_cell(repo_name.as_deref().is_some_and(looks_name_ok)),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "global user.name",
            ok_cell(global_name.as_deref().is_some_and(looks_name_ok)),
            label_w = label_w,
            name_w = name_w
        );
        let eff_name_disp = effective_name
            .as_deref()
            .map(blue)
            .unwrap_or_else(|| "(unset)".to_string());
        let eff_name_ok = effective_name.as_deref().is_some_and(looks_name_ok);
        eprintln!(
            "  {:<label_w$} {:<name_w$} {} ({})",
            "",
            "effective author name",
            ok_cell(eff_name_ok),
            eff_name_disp,
            label_w = label_w,
            name_w = name_w
        );

        // Identity rows: email
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "repo user.email",
            ok_cell(repo_email.as_deref().is_some_and(looks_email_ok)),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "global user.email",
            ok_cell(global_email.as_deref().is_some_and(looks_email_ok)),
            label_w = label_w,
            name_w = name_w
        );
        let eff_mail_disp = effective_email
            .as_deref()
            .map(blue)
            .unwrap_or_else(|| "(unset)".to_string());
        let eff_mail_ok = effective_email.as_deref().is_some_and(looks_email_ok);
        eprintln!(
            "  {:<label_w$} {:<name_w$} {} ({})",
            "",
            "effective author email",
            ok_cell(eff_mail_ok),
            eff_mail_disp,
            label_w = label_w,
            name_w = name_w
        );

        // Show env overrides presence
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "environment:",
            "GIT_AUTHOR_NAME",
            env_cell(env_name.is_some(), !eff_name_ok),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "GIT_AUTHOR_EMAIL",
            env_cell(env_email.is_some(), !eff_mail_ok),
            label_w = label_w,
            name_w = name_w
        );

        // Signing diagnostics
        let desired_signing = match std::env::var("AIFO_CODER_GIT_SIGN") {
            Ok(v) => {
                let vl = v.to_lowercase();
                !matches!(vl.as_str(), "0" | "false" | "no" | "off")
            }
            Err(_) => true, // default desire: enabled
        };
        let repo_sign = git_get_repo("commit.gpgsign");
        let global_sign = git_get_global("commit.gpgsign");
        let sign_effective = repo_sign.clone().or(global_sign.clone());

        let repo_key = git_get_repo("user.signingkey");
        let global_key = git_get_global("user.signingkey");
        let key_effective = repo_key.clone().or(global_key.clone());

        // Secret key availability via gpg
        let secret_keys_available = Command::new("gpg")
            .args(["--list-secret-keys", "--with-colons"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("\nfpr:"))
            .unwrap_or(false);

        let yesno = |b: bool| if b { "yes" } else { "no" };
        let sign_disp = |v: &Option<String>| {
            v.as_ref()
                .map(|s| s.to_lowercase())
                .map(|s| {
                    if s == "true" {
                        "true".to_string()
                    } else if s == "false" {
                        "false".to_string()
                    } else {
                        s
                    }
                })
                .unwrap_or_else(|| "(unset)".to_string())
        };

        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "git signing:",
            "desired",
            blue(yesno(desired_signing)),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "commit.gpgsign (repo)",
            blue(&sign_disp(&repo_sign)),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "commit.gpgsign (global)",
            blue(&sign_disp(&global_sign)),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "commit.gpgsign (effective)",
            blue(&sign_disp(&sign_effective)),
            label_w = label_w,
            name_w = name_w
        );

        let key_eff_disp = key_effective
            .as_deref()
            .map(blue)
            .unwrap_or_else(|| "(unset)".to_string());
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "signing key (effective)",
            key_eff_disp,
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "secret keys available",
            blue(yesno(secret_keys_available)),
            label_w = label_w,
            name_w = name_w
        );

        if verbose {
            // Tips
            let sign_eff_true = sign_effective
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            if desired_signing && !sign_eff_true {
                eprintln!("    tip: Signing desired but commit.gpgsign is not 'true'. Set it at repo or global level:");
                eprintln!("    tip:   git config commit.gpgsign true    # in repo");
                eprintln!("    tip:   git config --global commit.gpgsign true");
            }
            if desired_signing && key_effective.is_none() && secret_keys_available {
                eprintln!("    tip: A secret key is available but user.signingkey is unset. Configure it to your fingerprint:");
                eprintln!(
                    "    tip:   git config user.signingkey <FINGERPRINT>    # in repo or --global"
                );
            }
            if desired_signing && !secret_keys_available {
                eprintln!("    tip: No GPG secret keys found. Create or import a key, then set user.signingkey if needed.");
            }
            if !desired_signing && sign_eff_true {
                eprintln!("    tip: Signing disabled by AIFO_CODER_GIT_SIGN=0 but repo enables it. Disable in repo if undesired:");
                eprintln!("    tip:   git config commit.gpgsign false");
            }
        }
    } else {
        eprintln!("  git: (not found)");
        if verbose {
            eprintln!("    tip: Install Git and ensure 'git' is in your PATH.");
        }
    }
    eprintln!();

    // Aider files
    show("aider config:", home.join(".aider.conf.yml"), mount_aider);
    show(
        "aider metadata:",
        home.join(".aider.model.metadata.json"),
        mount_aider,
    );
    show(
        "aider settings:",
        home.join(".aider.model.settings.yml"),
        mount_aider,
    );
    eprintln!();

    // Crush paths
    show(
        "crush config:",
        home.join(".local").join("share").join("crush"),
        mount_crush,
    );
    show("crush state:", home.join(".crush"), mount_crush);
    eprintln!();

    // Codex path
    show("codex config:", home.join(".codex"), mount_codex);
    eprintln!();

    // AIFO API environment variables availability
    {
        let use_color = atty::is(atty::Stream::Stderr);
        let icon = |present: bool| -> String {
            if present {
                if use_color {
                    "\x1b[32m✅ found\x1b[0m".to_string()
                } else {
                    "✅ found".to_string()
                }
            } else if use_color {
                "\x1b[31m❌ missing\x1b[0m".to_string()
            } else {
                "❌ missing".to_string()
            }
        };
        let present = |name: &str| {
            std::env::var(name)
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
        };
        let has_key = present("AIFO_API_KEY");
        let has_base = present("AIFO_API_BASE");
        let has_version = present("AIFO_API_VERSION");

        let label_w: usize = 16;
        let name_w: usize = 44;
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "environment:",
            "AIFO_API_KEY",
            icon(has_key),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "AIFO_API_BASE",
            icon(has_base),
            label_w = label_w,
            name_w = name_w
        );
        eprintln!(
            "  {:<label_w$} {:<name_w$} {}",
            "",
            "AIFO_API_VERSION",
            icon(has_version),
            label_w = label_w,
            name_w = name_w
        );
    }
    eprintln!();

    // Workspace write test to validate mounts and UID mapping
    if aifo_coder::container_runtime_path().is_ok() {
        let image = default_image_for_quiet("crush");
        let tmpname = format!(
            ".aifo-coder-doctor-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        );
        let pwd = match std::env::current_dir() {
            Ok(p) => std::fs::canonicalize(&p).unwrap_or(p),
            Err(_) => PathBuf::from("."),
        };
        let uid = Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "0".to_string());
        let gid = Command::new("id")
            .arg("-g")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "0".to_string());

        // Run a short-lived container to validate workspace mount writeability; silence its output
        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("--rm")
            .arg("--user")
            .arg(format!("{uid}:{gid}"))
            .arg("-v")
            .arg(format!("{}:/workspace", pwd.display()))
            .arg("-w")
            .arg("/workspace")
            .arg("-e")
            .arg("HOME=/home/coder")
            .arg("-e")
            .arg("GNUPGHOME=/home/coder/.gnupg")
            .arg(&image)
            .arg("sh")
            .arg("-lc")
            .arg(format!(
                "echo ok > /workspace/{tmp} && id -u > /workspace/{tmp}.uid",
                tmp = tmpname
            ));
        let _ = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status();

        let host_file = pwd.join(&tmpname);
        let host_uid_file = pwd.join(format!("{tmp}.uid", tmp = tmpname));
        if host_file.exists() && host_uid_file.exists() {
            // Present readiness line aligned with the first status column (found/missing)
            let use_color = atty::is(atty::Stream::Stderr);
            let label_width: usize = 16;
            let path_col: usize = 52;
            let yes_val = if use_color {
                "\x1b[34;1myes\x1b[0m".to_string()
            } else {
                "yes".to_string()
            };
            let status_plain = "✅ workspace ready".to_string();
            let status_colored = if use_color {
                format!("\x1b[32m{}\x1b[0m", status_plain)
            } else {
                status_plain
            };
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
            // Fallback: if docker check failed, try host write test to confirm workspace directory is writable
            let host_write_ok = fs::write(&host_file, b"ok\n").is_ok()
                && fs::write(&host_uid_file, format!("{}\n", uid)).is_ok();
            if host_write_ok {
                // Present readiness line aligned with the first status column (found/missing)
                let use_color = atty::is(atty::Stream::Stderr);
                let label_width: usize = 16;
                let path_col: usize = 52;
                let yes_val = if use_color {
                    "\x1b[34;1myes\x1b[0m".to_string()
                } else {
                    "yes".to_string()
                };
                let status_plain = "✅ workspace ready".to_string();
                let status_colored = if use_color {
                    format!("\x1b[32m{}\x1b[0m", status_plain)
                } else {
                    status_plain
                };
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
                // On failure, report clearly without polluting stderr with container logs
                let use_color = atty::is(atty::Stream::Stderr);
                let label_width: usize = 16;
                let path_col: usize = 44;
                let no_val = if use_color {
                    "\x1b[34;1mno\x1b[0m".to_string()
                } else {
                    "no".to_string()
                };
                let status_plain = "❌ workspace not writable".to_string();
                let status_colored = if use_color {
                    format!("\x1b[31m{}\x1b[0m", status_plain)
                } else {
                    status_plain
                };
                eprintln!(
                    "  {:label_width$} {:<path_col$} {}",
                    "workspace writable:",
                    no_val,
                    status_colored,
                    label_width = label_width,
                    path_col = path_col
                );
            }
        }
    }

    eprintln!();
    eprintln!("doctor: completed diagnostics.");
    eprintln!();
}
