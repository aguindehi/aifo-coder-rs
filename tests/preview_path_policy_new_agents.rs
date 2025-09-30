/*
ignore-tidy-linelength
*/

#[test]
fn test_path_policy_and_naming_openhands_opencode_plandex() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Use a tiny image for preview (no pulls; deterministic)
    let img = "alpine:3.20";
    let args = vec!["--help".to_string()];

    // openhands: shims-first PATH and container naming includes agent
    let (_cmd_oh, preview_oh) =
        aifo_coder::build_docker_cmd("openhands", &args, img, None).expect("build_docker_cmd");
    assert!(
        preview_oh.contains("export PATH=\"/opt/aifo/bin:"),
        "openhands PATH should be shims-first; preview:\n{}",
        preview_oh
    );
    assert!(
        preview_oh.contains("--name ")
            && (preview_oh.contains("aifo-coder-openhands-")
                || preview_oh.contains("aifo-coder-opencode-")),
        "container name should include agent string; preview:\n{}",
        preview_oh
    );
    assert!(
        preview_oh.contains("--hostname ")
            && (preview_oh.contains("aifo-coder-openhands-")
                || preview_oh.contains("aifo-coder-opencode-")),
        "hostname should include agent string; preview:\n{}",
        preview_oh
    );

    // opencode: shims-first PATH and container naming includes agent
    let (_cmd_oc, preview_oc) =
        aifo_coder::build_docker_cmd("opencode", &args, img, None).expect("build_docker_cmd");
    assert!(
        preview_oc.contains("export PATH=\"/opt/aifo/bin:"),
        "opencode PATH should be shims-first; preview:\n{}",
        preview_oc
    );
    assert!(
        preview_oc.contains("--name ") && preview_oc.contains("aifo-coder-opencode-"),
        "container name should include agent string; preview:\n{}",
        preview_oc
    );
    assert!(
        preview_oc.contains("--hostname ") && preview_oc.contains("aifo-coder-opencode-"),
        "hostname should include agent string; preview:\n{}",
        preview_oc
    );

    // plandex: shims-first PATH and container naming includes agent
    let (_cmd_pl, preview_pl) =
        aifo_coder::build_docker_cmd("plandex", &args, img, None).expect("build_docker_cmd");
    assert!(
        preview_pl.contains("export PATH=\"/opt/aifo/bin:"),
        "plandex PATH should be shims-first; preview:\n{}",
        preview_pl
    );
    assert!(
        preview_pl.contains("--name ") && preview_pl.contains("aifo-coder-plandex-"),
        "container name should include agent string; preview:\n{}",
        preview_pl
    );
    assert!(
        preview_pl.contains("--hostname ") && preview_pl.contains("aifo-coder-plandex-"),
        "hostname should include agent string; preview:\n{}",
        preview_pl
    );
}
