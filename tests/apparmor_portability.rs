#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn test_apparmor_portability_non_linux() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let supported = aifo_coder::docker_supports_apparmor();
    let prof = aifo_coder::desired_apparmor_profile_quiet();
    if supported {
        assert_eq!(
            prof.as_deref(),
            Some("docker-default"),
            "on macOS/Windows with AppArmor support, expect docker-default"
        );
    } else {
        assert!(
            prof.is_none(),
            "when AppArmor unsupported by daemon, desired profile should be None"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn test_apparmor_portability_linux_nonflaky() {
    // This check should never panic and may validly return Some(aifo-coder), Some(docker-default) or None
    let _ = aifo_coder::container_runtime_path(); // ok if missing; function below handles it
    let prof = aifo_coder::desired_apparmor_profile_quiet();
    if let Some(p) = prof.as_deref() {
        assert!(
            p == "aifo-coder" || p == "docker-default",
            "unexpected Linux AppArmor profile: {}",
            p
        );
    } else {
        eprintln!(
            "AppArmor not in use or not supported; returning None is acceptable on this host"
        );
    }
}
