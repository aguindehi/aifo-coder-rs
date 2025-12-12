#[cfg(test)]
mod int_home_writability_agents {
    use std::path::PathBuf;
    use std::process::Command;

    fn docker_runtime() -> Option<PathBuf> {
        aifo_coder::container_runtime_path().ok()
    }

    fn skip_docker_tests() -> bool {
        std::env::var("AIFO_CODER_TEST_DISABLE_DOCKER")
            .ok()
            .as_deref()
            == Some("1")
    }

    fn image_present(image: &str) -> bool {
        let Some(runtime) = docker_runtime() else {
            eprintln!("docker not available; skipping image check: {}", image);
            return true; // treat as skipped when docker missing
        };
        Command::new(runtime)
            .arg("image")
            .arg("inspect")
            .arg(image)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn run_writability_check(image: &str) -> (bool, String) {
        let Some(runtime) = docker_runtime() else {
            eprintln!("docker not available; skipping: {}", image);
            return (true, String::new()); // treat as skipped
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
echo "probe: uid=$(id -u) gid=$(id -g) umask=$(umask)"

fail=0

diag() {
  op="$1"; p="$2"
  echo "FAIL ${op}: ${p}"
  echo "  uid:gid=$(id -u):$(id -g) umask=$(umask)"
  printf "  path stat: "
  stat -c '%u:%g %a' "$p" 2>/dev/null || echo 'N/A'
  echo "  path ls: $(ls -ld "$p" 2>&1 || echo 'N/A')"
  par="$(dirname "$p")"
  printf "  parent stat: "
  stat -c '%u:%g %a' "$par" 2>/dev/null || echo 'N/A'
  echo "  parent ls: $(ls -ld "$par" 2>&1 || echo 'N/A')"
  echo "  expected: $HOME mode 1777; subtrees 0777"
}

check_mkdir() {
  d="$1"
  if mkdir -p "$d/test.$$" >/dev/null 2>&1; then
    rmdir "$d/test.$$" >/dev/null 2>&1 || true
    return 0
  fi
  return 1
}

check_touch() {
  d="$1"
  f="$d/.writetest.$$"
  if : > "$f" >/dev/null 2>&1; then
    rm -f "$f" >/dev/null 2>&1 || true
    return 0
  fi
  return 1
}

paths="$HOME/.local $HOME/.local/share $HOME/.local/state $HOME/.local/share/uv $HOME/.local/share/pnpm $HOME/.cache"
for p in $paths; do
  if ! check_mkdir "$p"; then
    diag "mkdir" "$p"
    fail=1
  fi
  if ! check_touch "$p"; then
    diag "touch" "$p"
    fail=1
  fi
done

if [ "$fail" -eq 0 ]; then
  echo "ok"
else
  exit 1
fi
"#;

        let mut cmd = Command::new(runtime);
        cmd.arg("run").arg("--rm");
        if let Some((uid, gid)) = uidgid {
            cmd.arg("-u").arg(format!("{uid}:{gid}"));
        }
        cmd.arg(image).arg("sh").arg("-lc").arg(script);

        match cmd.output() {
            Ok(out) => {
                let mut s = String::new();
                s.push_str(&String::from_utf8_lossy(&out.stdout));
                if !out.stderr.is_empty() {
                    s.push_str("\n--- stderr ---\n");
                    s.push_str(&String::from_utf8_lossy(&out.stderr));
                }
                (out.status.success(), s)
            }
            Err(e) => (false, format!("failed to run docker: {}", e)),
        }
    }

    fn img(agent: &str) -> String {
        let prefix = std::env::var("IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
        let tag = std::env::var("TAG").unwrap_or_else(|_| "latest".to_string());
        format!("{prefix}-{agent}:{tag}")
    }

    fn check_image(image: &str) {
        if skip_docker_tests() {
            eprintln!("AIFO_CODER_TEST_DISABLE_DOCKER=1; skipping {}", image);
            return;
        }
        if !image_present(image) {
            eprintln!("image not present locally; skipping {}", image);
            return;
        }
        let (ok, out) = run_writability_check(image);
        assert!(
            ok,
            "HOME subtree writability failed for image: {}\n{}",
            image, out
        );
    }

    #[test]
    fn int_home_writability_codex() {
        check_image(&img("codex"));
    }

    #[test]
    fn int_home_writability_codex_slim() {
        check_image(&img("codex-slim"));
    }

    #[test]
    fn int_home_writability_crush() {
        check_image(&img("crush"));
    }

    #[test]
    fn int_home_writability_crush_slim() {
        check_image(&img("crush-slim"));
    }

    #[test]
    fn int_home_writability_aider() {
        check_image(&img("aider"));
    }

    #[test]
    fn int_home_writability_aider_slim() {
        check_image(&img("aider-slim"));
    }

    #[test]
    fn int_home_writability_openhands() {
        check_image(&img("openhands"));
    }

    #[test]
    fn int_home_writability_openhands_slim() {
        check_image(&img("openhands-slim"));
    }

    #[test]
    fn int_home_writability_opencode() {
        check_image(&img("opencode"));
    }

    #[test]
    fn int_home_writability_opencode_slim() {
        check_image(&img("opencode-slim"));
    }

    #[test]
    fn int_home_writability_plandex() {
        check_image(&img("plandex"));
    }

    #[test]
    fn int_home_writability_plandex_slim() {
        check_image(&img("plandex-slim"));
    }
}
