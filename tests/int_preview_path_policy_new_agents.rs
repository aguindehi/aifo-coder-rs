/*
ignore-tidy-linelength
*/

#[test]
fn int_test_path_policy_and_naming_openhands_opencode_plandex() {
    // Isolate HOME so preview mount discovery stays fast and deterministic
    let td = tempfile::tempdir().expect("tmpdir");
    let old_home = std::env::var("HOME").ok();
    std::env::set_var("HOME", td.path());

    // Use a tiny image for preview (no pulls; deterministic)
    let img = "alpine:3.20";
    let args = vec!["--help".to_string()];

    // openhands: shims-first PATH and container naming includes agent
    let preview_oh = aifo_coder::build_docker_preview_only("openhands", &args, img, None);
    assert!(
        preview_oh.contains("export PATH=\"/opt/aifo/bin:"),
        "openhands PATH should be shims-first; preview:\n{}",
        preview_oh
    );
    assert!(
        preview_oh.contains("--name ") && preview_oh.contains("aifo-coder-openhands-"),
        "container name should include agent string; preview:\n{}",
        preview_oh
    );
    assert!(
        preview_oh.contains("--hostname ") && preview_oh.contains("aifo-coder-openhands-"),
        "hostname should include agent string; preview:\n{}",
        preview_oh
    );

    // opencode: shims-first PATH and container naming includes agent
    let preview_oc = aifo_coder::build_docker_preview_only("opencode", &args, img, None);
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
    let preview_pl = aifo_coder::build_docker_preview_only("plandex", &args, img, None);
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

    // Restore HOME
    if let Some(v) = old_home {
        std::env::set_var("HOME", v);
    } else {
        std::env::remove_var("HOME");
    }
}
