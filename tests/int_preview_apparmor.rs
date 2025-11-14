#[test]
fn int_test_agent_preview_includes_apparmor_flag_when_supported() {
    // Requires docker and AppArmor support
    if aifo_coder::container_runtime_path().is_err() || !aifo_coder::docker_supports_apparmor() {
        eprintln!("skipping: docker not found or AppArmor unsupported");
        return;
    }
    let profile = aifo_coder::desired_apparmor_profile().expect("expected some apparmor profile");
    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", Some(&profile))
            .expect("build_docker_cmd failed");
    assert!(
        preview.contains(&format!("--security-opt apparmor={}", profile)),
        "preview missing AppArmor flag: {preview}"
    );
}

#[test]
fn int_test_agent_preview_omits_apparmor_flag_when_unsupported() {
    // Requires docker but with AppArmor unsupported
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found");
        return;
    }
    if aifo_coder::docker_supports_apparmor() {
        eprintln!("skipping: AppArmor supported on this host");
        return;
    }
    let args = vec!["--help".to_string()];
    let (_cmd, preview) =
        aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", Some("docker-default"))
            .expect("build_docker_cmd failed");
    assert!(
        !preview.contains("--security-opt apparmor="),
        "preview should not include AppArmor flag when unsupported: {preview}"
    );
}
