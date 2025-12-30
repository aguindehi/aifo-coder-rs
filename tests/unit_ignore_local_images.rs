#![cfg(unix)]

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn make_stub(runtime_dir: &Path, body: &str) -> std::path::PathBuf {
    let path = runtime_dir.join("docker");
    fs::write(&path, body).expect("write stub docker");
    let mut perms = fs::metadata(&path).expect("metadata").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub docker");
    path
}

#[test]
fn unit_ignore_local_images_hides_latest_and_pulled_tags() {
    // Save env to restore later
    let old_ignore = env::var("AIFO_CODER_IGNORE_LOCAL_IMAGES").ok();

    let td = tempfile::tempdir().expect("tmpdir");
    let runtime = make_stub(
        td.path(),
        r#"#!/bin/sh
exit 0
"#,
    );

    // Without ignore flag, both tags appear present.
    env::remove_var("AIFO_CODER_IGNORE_LOCAL_IMAGES");
    aifo_coder::set_ignore_local_images(false);
    let latest_present = aifo_coder::image_exists(&runtime, "aifo-coder-codex:latest");
    assert!(
        latest_present,
        "stub docker should report :latest as present"
    );

    let release_tag = format!("aifo-coder-codex:release-{}", env!("CARGO_PKG_VERSION"));
    let release_present = aifo_coder::image_exists(&runtime, &release_tag);
    assert!(
        release_present,
        "stub docker should report pulled version tag as present"
    );

    // With ignore-local-images, both tags must be hidden.
    env::set_var("AIFO_CODER_IGNORE_LOCAL_IMAGES", "1");
    aifo_coder::set_ignore_local_images(false);
    let latest_hidden = aifo_coder::image_exists(&runtime, "aifo-coder-codex:latest");
    assert!(
        !latest_hidden,
        "--ignore-local-images must skip local :latest images"
    );
    let release_hidden = aifo_coder::image_exists(&runtime, &release_tag);
    assert!(
        !release_hidden,
        "--ignore-local-images must skip pulled version-tagged images"
    );

    // Restore env
    if let Some(val) = old_ignore {
        env::set_var("AIFO_CODER_IGNORE_LOCAL_IMAGES", val);
    } else {
        env::remove_var("AIFO_CODER_IGNORE_LOCAL_IMAGES");
    }
    aifo_coder::set_ignore_local_images(false);
}
