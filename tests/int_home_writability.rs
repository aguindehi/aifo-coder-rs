#[cfg(test)]
mod int_home_writability {
    use std::process::Command;

    fn docker_runtime() -> Option<std::path::PathBuf> {
        aifo_coder::container_runtime_path().ok()
    }

    fn skip_docker_tests() -> bool {
        std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
            .ok()
            .as_deref()
            == Some("1")
    }

    fn run_writability_check(image: &str) -> bool {
        let Some(runtime) = docker_runtime() else {
            eprintln!("docker not available; skipping: {}", image);
            return true; // treat as skipped
        };

        #[cfg(unix)]
        let uidgid = {
            use nix::unistd::{getgid, getuid};
            Some((u32::from(getuid()), u32::from(getgid())))
        };
        #[cfg(not(unix))]
        let uidgid: Option<(u32, u32)> = None;

        let script = r#"set -eu
: "${HOME:=/home/coder}"

check_mkdir() {
  d="$1"
  mkdir -p "$d/test.$$" >/dev/null 2>&1 && rmdir "$d/test.$$" >/dev/null 2>&1
}

check_touch() {
  d="$1"
  f="$d/.writetest.$$"
  : > "$f" >/dev/null 2>&1 && rm -f "$f" >/dev/null 2>&1
}

paths="$HOME/.local $HOME/.local/share $HOME/.local/state $HOME/.local/share/uv $HOME/.local/share/pnpm $HOME/.cache"
for p in $paths; do
  check_mkdir "$p" || { echo "not writable: $p" >&2; exit 1; }
done
check_touch "$HOME/.local/share" || { echo "not writable (touch): $HOME/.local/share" >&2; exit 1; }
echo "ok"
"#;

        let mut cmd = Command::new(runtime);
        cmd.arg("run").arg("--rm");
        if let Some((uid, gid)) = uidgid {
            cmd.arg("-u").arg(format!("{uid}:{gid}"));
        }
        cmd.arg(image)
            .arg("sh")
            .arg("-lc")
            .arg(script);

        match cmd.status() {
            Ok(st) => st.success(),
            Err(_) => false,
        }
    }

    #[test]
    fn int_home_writability_full() {
        if skip_docker_tests() {
            eprintln!("AIFO_CODER_TEST_DISABLE_DOCKER=1; skipping");
            return;
        }
        let prefix = std::env::var("IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
        let tag = std::env::var("TAG").unwrap_or_else(|_| "latest".to_string());
        let image = format!("{prefix}-aider:{tag}");
        assert!(
            run_writability_check(&image),
            "HOME subtree writability failed for image: {}",
            image
        );
    }

    #[test]
    fn int_home_writability_slim() {
        if skip_docker_tests() {
            eprintln!("AIFO_CODER_TEST_DISABLE_DOCKER=1; skipping");
            return;
        }
        let prefix = std::env::var("IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
        let tag = std::env::var("TAG").unwrap_or_else(|_| "latest".to_string());
        let image = format!("{prefix}-aider-slim:{tag}");
        assert!(
            run_writability_check(&image),
            "HOME subtree writability failed for image: {}",
            image
        );
    }
}
