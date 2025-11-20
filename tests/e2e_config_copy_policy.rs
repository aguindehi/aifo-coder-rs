#![allow(clippy::manual_assert)]
// ignore-tidy-linelength

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn docker() -> Option<PathBuf> {
    aifo_coder::container_runtime_path().ok()
}

fn image_for_aider() -> Option<String> {
    // Prefer explicit env, else default prefix-tag
    if let Ok(img) = env::var("AIDER_IMAGE") {
        let img = img.trim().to_string();
        if !img.is_empty() {
            return Some(img);
        }
    }
    // Fallback to typical default
    Some(format!(
        "{}-aider:{}",
        env::var("IMAGE_PREFIX")
            .ok()
            .unwrap_or_else(|| "aifo-coder".to_string()),
        env::var("TAG").ok().unwrap_or_else(|| "latest".to_string())
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

fn unique_name(prefix: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{}", prefix, now)
}

fn run_detached_sleep_container(
    runtime: &Path,
    image: &str,
    name: &str,
    host_cfg_dir: &Path,
    extra_env: &[(&str, &str)],
) -> bool {
    let mut args: Vec<String> = vec![
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
    ];
    for (k, v) in extra_env {
        args.push("-e".into());
        args.push(format!("{k}={v}"));
    }
    args.push(image.into());
    args.push("/bin/sleep".into());
    args.push("infinity".into());

    let mut cmd = Command::new(runtime);
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

fn exec_sh(runtime: &Path, name: &str, script: &str) -> (i32, String) {
    let mut cmd = Command::new(runtime);
    cmd.arg("exec").arg(name).arg("sh").arg("-c").arg(script);
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

fn stop_container(runtime: &Path, name: &str) {
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
fn e2e_config_copy_and_permissions_for_aider() {
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

    // Prepare host config root with allowed files
    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();
    let global = root.join("global");
    let aider = root.join("aider");
    fs::create_dir_all(&global).expect("mk global");
    fs::create_dir_all(&aider).expect("mk aider");

    fs::write(global.join("config.toml"), "k = \"v\"\n").expect("write config.toml");
    fs::write(aider.join(".aider.conf.yml"), "aider: conf\n").expect("write aider conf");
    fs::write(aider.join("creds.token"), "secret-token-123").expect("write token");

    // Run container detached with entrypoint copy-on-start
    let name = unique_name("aifo-e2e-config");
    let ok = run_detached_sleep_container(&runtime, &image, &name, root, &[]);
    assert!(ok, "failed to start container {}", name);

    // Verify files and permissions inside container
    let script = r#"
set -e
ok1=""; ok2=""; ok3=""; ok4=""; ok5=""
[ -f "$HOME/.aifo-config/global/config.toml" ] && ok1="1"
perm1="$(stat -c %a "$HOME/.aifo-config/global/config.toml" 2>/dev/null || stat -f %p "$HOME/.aifo-config/global/config.toml" 2>/dev/null | awk '{printf "%04o\n",$1}' || echo "")"
[ "$perm1" = "0644" ] && ok2="1"
[ -f "$HOME/.aifo-config/aider/.aider.conf.yml" ] && ok3="1"
[ -f "$HOME/.aifo-config/aider/creds.token" ] && {
  perm2="$(stat -c %a "$HOME/.aifo-config/aider/creds.token" 2>/dev/null || stat -f %p "$HOME/.aifo-config/aider/creds.token" 2>/dev/null | awk '{printf "%04o\n",$1}' || echo "")"
  [ "$perm2" = "0600" ] && ok4="1"
}
[ -f "$HOME/.aider.conf.yml" ] && ok5="1"
[ -f "$HOME/.aifo-config/.copied" ] && echo "STAMP=present" || echo "STAMP=missing"
echo "OKS=$ok1$ok2$ok3$ok4$ok5"
"#;
    let (_ec, out) = exec_sh(&runtime, &name, script);
    stop_container(&runtime, &name);

    assert!(
        out.contains("STAMP=present"),
        "stamp missing; output:\n{}",
        out
    );
    assert!(
        out.contains("OKS=11111"),
        "expected all checks to pass (presence + perms), got:\n{}",
        out
    );
}

#[test]
#[ignore]
fn e2e_config_skip_symlink_oversized_disallowed() {
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

    let td = tempfile::tempdir().expect("tmpdir");
    let root = td.path();
    let aider = root.join("aider");
    fs::create_dir_all(&aider).expect("mk aider");
    fs::write(aider.join("ok.yaml"), "ok: true\n").expect("write ok");

    // Disallowed ext
    fs::write(aider.join("unknown.xxx"), "nope").expect("write unknown");

    // Oversized file (> 256 KiB)
    let mut big = fs::File::create(aider.join("huge.yml")).expect("create huge");
    let buf = vec![b'a'; 300 * 1024];
    big.write_all(&buf).expect("write huge");

    // Symlink (best-effort; skip on platforms without symlink)
    #[cfg(unix)]
    std::os::unix::fs::symlink(aider.join("ok.yaml"), aider.join("link.yml")).ok();

    let name = unique_name("aifo-e2e-config");
    let ok = run_detached_sleep_container(
        &runtime,
        &image,
        &name,
        root,
        &[("AIFO_TOOLCHAIN_VERBOSE", "1")],
    );
    assert!(ok, "failed to start container {}", name);

    let script = r#"
set -e
have_ok=0; have_unknown=0; have_huge=0; have_link=0
[ -f "$HOME/.aifo-config/aider/ok.yaml" ] && have_ok=1
[ -f "$HOME/.aifo-config/aider/unknown.xxx" ] && have_unknown=1
[ -f "$HOME/.aifo-config/aider/huge.yml" ] && have_huge=1
[ -f "$HOME/.aifo-config/aider/link.yml" ] && have_link=1
echo "RES=$have_ok/$have_unknown/$have_huge/$have_link"
"#;
    let (_ec, out) = exec_sh(&runtime, &name, script);
    stop_container(&runtime, &name);

    assert!(
        out.contains("RES=1/0/0/0"),
        "expected only ok.yaml copied; out:\n{}",
        out
    );
}
