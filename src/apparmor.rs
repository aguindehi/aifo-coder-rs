#![allow(clippy::module_name_repetitions)]
//! AppArmor detection and profile selection helpers.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::warn_print;
#[cfg(feature = "otel")]
use tracing::instrument;

#[cfg_attr(feature = "otel", instrument(level = "debug"))]
pub fn docker_supports_apparmor() -> bool {
    let runtime = match crate::container_runtime_path() {
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
    if cfg!(target_os = "linux") && !kernel_apparmor_enabled() {
        return false;
    }
    true
}

fn kernel_apparmor_enabled() -> bool {
    if let Ok(content) = fs::read_to_string("/sys/module/apparmor/parameters/enabled") {
        let c = content.trim().to_lowercase();
        if c.starts_with('y')
            || c.contains("enforce")
            || c.contains("complain")
            || c == "1"
            || c == "yes"
            || c == "true"
        {
            return Path::new("/proc/self/attr/apparmor/current").exists()
                && Path::new("/proc/self/attr/apparmor/exec").exists();
        } else {
            return false;
        }
    }
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

#[cfg_attr(
    feature = "otel",
    instrument(level = "debug", skip(), fields(aifo_coder_source = "noisy"))
)]
pub fn desired_apparmor_profile() -> Option<String> {
    if !docker_supports_apparmor() {
        return None;
    }
    if let Ok(p) = env::var("AIFO_CODER_APPARMOR_PROFILE") {
        let trimmed = p.trim();
        let lower = trimmed.to_lowercase();
        if trimmed.is_empty()
            || ["none", "no", "off", "false", "0", "disabled", "disable"].contains(&lower.as_str())
        {
            return None;
        }
        if cfg!(target_os = "linux") && !apparmor_profile_available(trimmed) {
            warn_print(&format!(
                "apparmor profile '{}' not loaded on host; falling back to 'docker-default'.",
                trimmed
            ));
            if apparmor_profile_available("docker-default") {
                return Some("docker-default".to_string());
            } else {
                warn_print("'docker-default' profile not found; continuing without explicit AppArmor profile.");
                return None;
            }
        }
        return Some(trimmed.to_string());
    }
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        Some("docker-default".to_string())
    } else if apparmor_profile_available("aifo-coder") {
        Some("aifo-coder".to_string())
    } else if apparmor_profile_available("docker-default") {
        warn_print("apparmor profile 'aifo-coder' not loaded; using 'docker-default'.");
        Some("docker-default".to_string())
    } else {
        warn_print("no known apparmor profile loaded; continuing without explicit profile.");
        None
    }
}

#[cfg_attr(
    feature = "otel",
    instrument(level = "debug", skip(), fields(aifo_coder_source = "quiet"))
)]
pub fn desired_apparmor_profile_quiet() -> Option<String> {
    if !docker_supports_apparmor() {
        return None;
    }
    if let Ok(p) = env::var("AIFO_CODER_APPARMOR_PROFILE") {
        let trimmed = p.trim();
        let lower = trimmed.to_lowercase();
        if trimmed.is_empty()
            || ["none", "no", "off", "false", "0", "disabled", "disable"].contains(&lower.as_str())
        {
            return None;
        }
        if cfg!(target_os = "linux") && !apparmor_profile_available(trimmed) {
            if apparmor_profile_available("docker-default") {
                return Some("docker-default".to_string());
            } else {
                return None;
            }
        }
        return Some(trimmed.to_string());
    }
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        Some("docker-default".to_string())
    } else if apparmor_profile_available("aifo-coder") {
        Some("aifo-coder".to_string())
    } else if apparmor_profile_available("docker-default") {
        Some("docker-default".to_string())
    } else {
        None
    }
}
