#![allow(clippy::manual_assert)]
// ignore-tidy-linelength

mod support;

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::{thread, time::Duration};
use tempfile::Builder;

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

fn image_exists(runtime: &PathBuf, image: &str) -> bool {
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

fn run_detached_sleep_container(
    runtime: &PathBuf,
    image: &str,
    name: &str,
    host_cfg_dir: &std::path::Path,
) -> bool {
    let args: Vec<String> = vec![
        "docker".into(),
        "run".into(),
        "-d".into(),
        "--rm".into(),
        "--name".into(),
        name.into(),
        "-v".into(),
        format!(
            "{}:/home/coder/.aifo-config-host:ro",
            host_cfg_dir.display()
        ),
        "-e".into(),
        "AIFO_CONFIG_HOST_DIR=/home/coder/.aifo-config-host".into(),
        "-e".into(),
        "AIFO_CODER_CONFIG_HOST_DIR=/home/coder/.aifo-config-host".into(),
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
fn e2e_config_concurrent_isolation() {
    let runtime = match docker() {
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

    // Host config with a simple aider file to trigger aider dir creation
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or(env::temp_dir());
    let td = Builder::new()
        .prefix("aifo-e2e-")
        .tempdir_in(&home)
        .expect("tmpdir in HOME");
    let root = td.path();
    std::fs::create_dir_all(root.join("aider")).expect("mk aider");
    std::fs::write(
        root.join("aider").join(".aider.model.settings.yml"),
        "model: x",
    )
    .expect("write model settings");

    let name1 = support::unique_name("aifo-e2e-cfg");
    let name2 = support::unique_name("aifo-e2e-cfg");

    assert!(run_detached_sleep_container(&runtime, &image, &name1, root));
    assert!(run_detached_sleep_container(&runtime, &image, &name2, root));

    // Wait for config readiness in container 1
    let mut ready1 = support::wait_for_config_copied(runtime.as_path(), &name1);
    if !ready1 {
        let _ = exec_sh(
            &runtime,
            &name1,
            r#"/usr/local/bin/aifo-entrypoint /bin/true || true"#,
        );
        ready1 = support::wait_for_config_copied(runtime.as_path(), &name1);
    }
    assert!(ready1, "config not ready in {}", name1);

    // Wait for config readiness in container 2
    let mut ready2 = support::wait_for_config_copied(runtime.as_path(), &name2);
    if !ready2 {
        let _ = exec_sh(
            &runtime,
            &name2,
            r#"/usr/local/bin/aifo-entrypoint /bin/true || true"#,
        );
        ready2 = support::wait_for_config_copied(runtime.as_path(), &name2);
    }
    assert!(ready2, "config not ready in {}", name2);

    // Create distinct markers in each container's private copy dir
    let (_ec1, _out1) = support::docker_exec_sh(
        runtime.as_path(),
        &name1,
        r#"set -e; echo "alpha" > "$HOME/.aifo-config/isolation.txt"; cat "$HOME/.aifo-config/isolation.txt""#,
    );
    let (_ec2, _out2) = support::docker_exec_sh(
        runtime.as_path(),
        &name2,
        r#"set -e; echo "beta" > "$HOME/.aifo-config/isolation.txt"; cat "$HOME/.aifo-config/isolation.txt""#,
    );

    // Verify isolation: name1 sees alpha; name2 sees beta
    let (_ec1b, out1b) = support::docker_exec_sh(
        runtime.as_path(),
        &name1,
        r#"set -e; cat "$HOME/.aifo-config/isolation.txt" || echo missing"#,
    );
    let (_ec2b, out2b) = support::docker_exec_sh(
        runtime.as_path(),
        &name2,
        r#"set -e; cat "$HOME/.aifo-config/isolation.txt" || echo missing"#,
    );

    // Verify Aider bridging exists in each container
    let (_ecb1, bridge1) = support::docker_exec_sh(
        runtime.as_path(),
        &name1,
        r#"set -e; if [ -f "$HOME/.aider.model.settings.yml" ]; then echo "BRIDGE=ok"; else echo "BRIDGE=miss"; fi"#,
    );
    let (_ecb2, bridge2) = support::docker_exec_sh(
        runtime.as_path(),
        &name2,
        r#"set -e; if [ -f "$HOME/.aider.model.settings.yml" ]; then echo "BRIDGE=ok"; else echo "BRIDGE=miss"; fi"#,
    );

    assert!(
        out1b.contains("alpha"),
        "container1 should see alpha; got:\n{}",
        out1b
    );
    assert!(
        out2b.contains("beta"),
        "container2 should see beta; got:\n{}",
        out2b
    );
    assert!(
        bridge1.contains("BRIDGE=ok"),
        "container1 missing Aider bridging file; out:\n{}",
        bridge1
    );
    assert!(
        bridge2.contains("BRIDGE=ok"),
        "container2 missing Aider bridging file; out:\n{}",
        bridge2
    );

    support::stop_container(runtime.as_path(), &name1);
    support::stop_container(runtime.as_path(), &name2);
}
