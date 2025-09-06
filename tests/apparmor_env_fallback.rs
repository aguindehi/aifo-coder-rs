#[cfg(target_os = "linux")]
#[test]
fn test_apparmor_env_fallback_to_docker_default_if_available() {
    // Only meaningful on Linux with Docker reporting AppArmor support
    if !aifo_coder::docker_supports_apparmor() {
        eprintln!("skipping: docker does not report AppArmor support");
        return;
    }
    // Set to a surely non-existent profile; implementation should fall back to docker-default if it's loaded
    let old = std::env::var("AIFO_CODER_APPARMOR_PROFILE").ok();
    std::env::set_var(
        "AIFO_CODER_APPARMOR_PROFILE",
        "surely-not-present-profile-xyz",
    );
    let prof = aifo_coder::desired_apparmor_profile_quiet();
    // Restore env
    if let Some(v) = old {
        std::env::set_var("AIFO_CODER_APPARMOR_PROFILE", v);
    } else {
        std::env::remove_var("AIFO_CODER_APPARMOR_PROFILE");
    }

    if let Some(p) = prof {
        assert_eq!(
            p, "docker-default",
            "expected fallback to docker-default, got {}",
            p
        );
    } else {
        eprintln!("skipping: docker-default not loaded on host; profile remained None");
    }
}
