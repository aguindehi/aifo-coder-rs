#![allow(clippy::manual_assert)]
// ignore-tidy-linelength

mod support;

use std::path::Path;
use std::process::{Command, Stdio};
use std::{thread, time::Duration};

fn image_for_aider() -> Option<String> {
    if let Ok(img) = std::env::var("AIDER_IMAGE") {
        let img = img.trim().to_string();
        if !img.is_empty() {
            return Some(img);
        }
    }
    Some(format!(
        "{}-aider:{}",
        std::env::var("IMAGE_PREFIX")
            .ok()
            .unwrap_or_else(|| "aifo-coder".to_string()),
        std::env::var("TAG")
            .ok()
            .unwrap_or_else(|| "latest".to_string())
    ))
}

fn image_exists(runtime: &Path, image: &str) -> bool {
    Command::new(runtime)
        .arg("image")
        .arg("inspect")
        .arg(image)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_detached_sleep_container_nomount(runtime: &Path, image: &str, name: &str) -> bool {
    let args: Vec<String> = vec![
        "docker".into(),
        "run".into(),
        "-d".into(),
        "--rm".into(),
        "--name".into(),
        name.into(),
        "-e".into(),
        "HOME=/home/coder".into(),
        "-e".into(),
        "GNUPGHOME=/home/coder/.gnupg".into(),
        image.into(),
        "/bin/sleep".into(),
        "infinity".into(),
    ];
    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

#[test]
#[ignore]
fn e2e_config_missing_host_dir_runtime_no_copy_stamp() {
    let runtime = match support::docker_runtime() {
        Some(p) => p,
        None => {
            eprintln!("skipping: docker runtime not available");
            return;
        }
    };
    let image = match image_for_aider() {
        Some(img) => img,
        None => {
            eprintln!("skipping: could not resolve agent image");
            return;
        }
    };
    if !image_exists(&runtime, &image) {
        eprintln!("skipping: image '{}' not present locally", image);
        return;
    }

    // Start aider without mounting a config host directory
    let name = support::unique_name("aifo-e2e-missing-host");
    assert!(
        run_detached_sleep_container_nomount(&runtime, &image, &name),
        "failed to start container {}",
        name
    );

    // Wait for $HOME/.aifo-config to be created (stamp should remain absent)
    let mut have_dir = false;
    for _ in 0..50 {
        let (_ec, out_ready) = support::docker_exec_sh(
            &runtime,
            &name,
            r#"if [ -d "$HOME/.aifo-config" ]; then echo READY; fi"#,
        );
        if out_ready.contains("READY") {
            have_dir = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !have_dir {
        let _ = support::docker_exec_sh(
            &runtime,
            &name,
            r#"/usr/local/bin/aifo-entrypoint /bin/true || true"#,
        );
        for _ in 0..50 {
            let (_ec, out_ready) = support::docker_exec_sh(
                &runtime,
                &name,
                r#"if [ -d "$HOME/.aifo-config" ]; then echo READY; fi"#,
            );
            if out_ready.contains("READY") {
                have_dir = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    assert!(
        have_dir,
        "expected $HOME/.aifo-config to be created by entrypoint or fallback"
    );

    // Inside container: $HOME/.aifo-config should exist; .copied stamp should be absent
    let script = r#"
set -e
d="$HOME/.aifo-config"
if [ -d "$d" ]; then echo "DST_DIR=present"; else echo "DST_DIR=missing"; fi
if [ -f "$d/.copied" ]; then echo "STAMP=present"; else echo "STAMP=absent"; fi
"#;
    let (_ec, out) = support::docker_exec_sh(&runtime, &name, script);
    support::stop_container(&runtime, &name);

    assert!(
        out.contains("DST_DIR=present"),
        "expected $HOME/.aifo-config to exist; got:\n{}",
        out
    );
    assert!(
        out.contains("STAMP=absent"),
        "expected .copied stamp to be absent when no host dir is mounted; got:\n{}",
        out
    );
}
