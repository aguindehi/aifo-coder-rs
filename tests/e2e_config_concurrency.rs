#![allow(clippy::manual_assert)]
// ignore-tidy-linelength

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn docker() -> Option<PathBuf> {
    aifo_coder::container_runtime_path().ok()
}

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

fn exec_sh(runtime: &PathBuf, name: &str, script: &str) -> (i32, String) {
    let mut cmd = Command::new(runtime);
    cmd.arg("exec").arg(name).arg("sh").arg("-lc").arg(script);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    match cmd.output() {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout).to_string()
                + &String::from_utf8_lossy(&o.stderr).to_string();
            (o.status.code().unwrap_or(1), out)
        }
        Err(e) => (1, format!("exec failed: {}", e)),
    }
}

fn stop_container(runtime: &PathBuf, name: &str) {
    let _ = Command::new(runtime)
        .arg("stop")
        .arg("--time")
        .arg("1")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
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
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();
    std::fs::create_dir_all(root.join("aider")).expect("mk aider");
    std::fs::write(
        root.join("aider").join(".aider.model.settings.yml"),
        "model: x",
    )
    .expect("write model settings");

    let name1 = format!(
        "aifo-e2e-cfg-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
    );
    let name2 = format!(
        "aifo-e2e-cfg-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros()
            + 1
    );

    assert!(run_detached_sleep_container(&runtime, &image, &name1, root));
    assert!(run_detached_sleep_container(&runtime, &image, &name2, root));

    // Create distinct markers in each container's private copy dir
    let (_ec1, _out1) = exec_sh(
        &runtime,
        &name1,
        r#"set -e; echo "alpha" > "$HOME/.aifo-config/isolation.txt"; cat "$HOME/.aifo-config/isolation.txt""#,
    );
    let (_ec2, _out2) = exec_sh(
        &runtime,
        &name2,
        r#"set -e; echo "beta" > "$HOME/.aifo-config/isolation.txt"; cat "$HOME/.aifo-config/isolation.txt""#,
    );

    // Verify isolation: name1 sees alpha; name2 sees beta
    let (_ec1b, out1b) = exec_sh(
        &runtime,
        &name1,
        r#"set -e; cat "$HOME/.aifo-config/isolation.txt" || echo missing"#,
    );
    let (_ec2b, out2b) = exec_sh(
        &runtime,
        &name2,
        r#"set -e; cat "$HOME/.aifo-config/isolation.txt" || echo missing"#,
    );

    // Verify Aider bridging exists in each container
    let (_ecb1, bridge1) = exec_sh(
        &runtime,
        &name1,
        r#"set -e; if [ -f "$HOME/.aider.model.settings.yml" ]; then echo "BRIDGE=ok"; else echo "BRIDGE=miss"; fi"#,
    );
    let (_ecb2, bridge2) = exec_sh(
        &runtime,
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

    stop_container(&runtime, &name1);
    stop_container(&runtime, &name2);
}
