use std::env;

#[test]
fn int_test_build_docker_cmd_passes_visual_term_tz() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let old_visual = env::var("VISUAL").ok();
    let old_term = env::var("TERM").ok();
    let old_tz = env::var("TZ").ok();
    env::set_var("VISUAL", "vim");
    env::set_var("TERM", "xterm-256color");
    env::set_var("TZ", "Europe/Zurich");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd");

    // Restore env
    if let Some(v) = old_visual {
        env::set_var("VISUAL", v);
    } else {
        env::remove_var("VISUAL");
    }
    if let Some(v) = old_term {
        env::set_var("TERM", v);
    } else {
        env::remove_var("TERM");
    }
    if let Some(v) = old_tz {
        env::set_var("TZ", v);
    } else {
        env::remove_var("TZ");
    }

    assert!(
        preview.contains("-e VISUAL"),
        "expected -e VISUAL in preview: {}",
        preview
    );
    assert!(
        preview.contains("-e TERM"),
        "expected -e TERM in preview: {}",
        preview
    );
    assert!(
        preview.contains("-e TZ"),
        "expected -e TZ in preview: {}",
        preview
    );
}
