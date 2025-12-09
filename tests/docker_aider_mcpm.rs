/*
// ignore-tidy-linelength
*/

use std::process::{Command, Stdio};

fn have_docker() -> bool {
    Command::new("docker")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("spawn failed: {e}"))?;
    if !out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout);
        let e = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "cmd failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
            out.status.code(),
            s,
            e
        ));
    }
    Ok(())
}

fn run_expect_fail(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("spawn failed: {e}"))?;
    if out.status.success() {
        return Err("expected failure but succeeded".to_string());
    }
    Ok(())
}

fn docker_build(target: &str, tag: &str, extra_args: &[&str]) -> Result<(), String> {
    let mut cmd = Command::new("docker");
    cmd.arg("build")
        .arg("--target")
        .arg(target)
        .arg("-t")
        .arg(tag);
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.arg(".")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run(&mut cmd)
}

fn docker_run(tag: &str, shell: &str, script: &str) -> Result<(), String> {
    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("--rm")
        .arg(tag)
        .arg(shell)
        .arg("-lc")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run(&mut cmd)
}

#[test]
fn integration_aider_mcpm_enabled_presence() {
    if std::env::var("AIFO_TEST_DOCKER").unwrap_or_default() != "1" || !have_docker() {
        eprintln!("skipping: docker tests disabled or docker unavailable");
        return;
    }
    let tag = "aifo-aider:mcpm1";
    docker_build("aider", tag, &[]).expect("docker build aider");
    docker_run(tag, "sh", "command -v mcpm-aider").expect("mcpm-aider in PATH");
    docker_run(tag, "sh", "mcpm-aider --version").expect("mcpm-aider runs");
    docker_run(tag, "sh", "command -v uv").expect("uv present");
    docker_run(tag, "sh", "grep -q '/usr/local/bin/node' /usr/local/bin/mcpm-aider")
        .expect("wrapper uses real node");
    docker_run(
        tag,
        "sh",
        "/opt/venv/bin/python -c 'import playwright; print(1)' | grep -q '^1$'",
    )
    .expect("playwright import ok");
}

#[test]
fn integration_aider_slim_mcpm_enabled_presence() {
    if std::env::var("AIFO_TEST_DOCKER").unwrap_or_default() != "1" || !have_docker() {
        eprintln!("skipping: docker tests disabled or docker unavailable");
        return;
    }
    let tag = "aifo-aider-slim:mcpm1";
    docker_build("aider-slim", tag, &[]).expect("docker build aider-slim");
    docker_run(tag, "sh", "command -v mcpm-aider").expect("mcpm-aider in PATH (slim)");
    docker_run(tag, "sh", "mcpm-aider --version").expect("mcpm-aider runs (slim)");
    docker_run(tag, "sh", "command -v uv").expect("uv present (slim)");
    docker_run(tag, "sh", "grep -q '/usr/local/bin/node' /usr/local/bin/mcpm-aider")
        .expect("wrapper uses real node (slim)");
    docker_run(
        tag,
        "sh",
        "/opt/venv/bin/python -c 'import playwright; print(1)' | grep -q '^1$'",
    )
    .expect("playwright import ok (slim)");
}

#[test]
fn integration_aider_mcpm_disabled_absence() {
    if std::env::var("AIFO_TEST_DOCKER").unwrap_or_default() != "1" || !have_docker() {
        eprintln!("skipping: docker tests disabled or docker unavailable");
        return;
    }
    let tag = "aifo-aider:mcpm0";
    docker_build("aider", tag, &["--build-arg", "WITH_MCPM_AIDER=0"]).expect("docker build aider (disabled)");
    // mcpm-aider should be absent
    run_expect_fail(
        Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg(tag)
            .arg("sh")
            .arg("-lc")
            .arg("command -v mcpm-aider"),
    )
    .expect("mcpm-aider absent");
    // real node should be removed
    run_expect_fail(
        Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg(tag)
            .arg("sh")
            .arg("-lc")
            .arg("test -x /usr/local/bin/node"),
    )
    .expect("node absent");
    // playwright still ok
    docker_run(
        tag,
        "sh",
        "/opt/venv/bin/python -c 'import playwright; print(1)' | grep -q '^1$'",
    )
    .expect("playwright import ok");
}

#[test]
fn integration_aider_slim_mcpm_disabled_absence() {
    if std::env::var("AIFO_TEST_DOCKER").unwrap_or_default() != "1" || !have_docker() {
        eprintln!("skipping: docker tests disabled or docker unavailable");
        return;
    }
    let tag = "aifo-aider-slim:mcpm0";
    docker_build("aider-slim", tag, &["--build-arg", "WITH_MCPM_AIDER=0"])
        .expect("docker build aider-slim (disabled)");
    // mcpm-aider should be absent
    run_expect_fail(
        Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg(tag)
            .arg("sh")
            .arg("-lc")
            .arg("command -v mcpm-aider"),
    )
    .expect("mcpm-aider absent (slim)");
    // real node should be removed
    run_expect_fail(
        Command::new("docker")
            .arg("run")
            .arg("--rm")
            .arg(tag)
            .arg("sh")
            .arg("-lc")
            .arg("test -x /usr/local/bin/node"),
    )
    .expect("node absent (slim)");
    // playwright still ok
    docker_run(
        tag,
        "sh",
        "/opt/venv/bin/python -c 'import playwright; print(1)' | grep -q '^1$'",
    )
    .expect("playwright import ok (slim)");
}
