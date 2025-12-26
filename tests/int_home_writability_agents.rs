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

        let script = aifo_coder::ShellFile::new()
            .extend([
                "set -eu".to_string(),
                r#": "${HOME:=/home/coder}""#.to_string(),
                r#"echo "probe: uid=$(id -u) gid=$(id -g) umask=$(umask)""#.to_string(),
                "".to_string(),
                "fail=0".to_string(),
                "".to_string(),
                "diag() {".to_string(),
                r#"  op="$1"; p="$2""#.to_string(),
                r#"  echo "FAIL ${op}: ${p}""#.to_string(),
                r#"  echo "  uid:gid=$(id -u):$(id -g) umask=$(umask)""#.to_string(),
                r#"  printf "  path stat: ""#.to_string(),
                r#"  stat -c '%u:%g %a' "$p" 2>/dev/null || echo 'N/A'"#.to_string(),
                r#"  echo "  path ls: $(ls -ld "$p" 2>&1 || echo 'N/A')""#.to_string(),
                r#"  par="$(dirname "$p")""#.to_string(),
                r#"  printf "  parent stat: ""#.to_string(),
                r#"  stat -c '%u:%g %a' "$par" 2>/dev/null || echo 'N/A'"#.to_string(),
                r#"  echo "  parent ls: $(ls -ld "$par" 2>&1 || echo 'N/A')""#.to_string(),
                r#"  echo "  expected: $HOME mode 1777; subtrees 0777; offending path: $p""#.to_string(),
                "}".to_string(),
                "".to_string(),
                "check_mkdir() {".to_string(),
                r#"  d="$1""#.to_string(),
                r#"  if mkdir -p "$d/test.$$" >/dev/null 2>&1; then"#.to_string(),
                r#"    rmdir "$d/test.$$" >/dev/null 2>&1 || true"#.to_string(),
                "    return 0".to_string(),
                "  fi".to_string(),
                "  return 1".to_string(),
                "}".to_string(),
                "".to_string(),
                "check_touch() {".to_string(),
                r#"  d="$1""#.to_string(),
                r#"  f="$d/.writetest.$$""#.to_string(),
                r#"  if : > "$f" >/dev/null 2>&1; then"#.to_string(),
                r#"    rm -f "$f" >/dev/null 2>&1 || true"#.to_string(),
                "    return 0".to_string(),
                "  fi".to_string(),
                "  return 1".to_string(),
                "}".to_string(),
                "".to_string(),
                r#"paths="$HOME/.local $HOME/.local/share $HOME/.local/state $HOME/.local/share/uv $HOME/.local/share/pnpm $HOME/.cache""#.to_string(),
                "for p in $paths; do".to_string(),
                r#"  if ! check_mkdir "$p"; then"#.to_string(),
                r#"    diag "mkdir" "$p""#.to_string(),
                "    fail=1".to_string(),
                "  fi".to_string(),
                r#"  if ! check_touch "$p"; then"#.to_string(),
                r#"    diag "touch" "$p""#.to_string(),
                "    fail=1".to_string(),
                "  fi".to_string(),
                "done".to_string(),
                "".to_string(),
                r#"if [ "$fail" -eq 0 ]; then"#.to_string(),
                r#"  echo "ok""#.to_string(),
                "else".to_string(),
                "  exit 1".to_string(),
                "fi".to_string(),
            ])
            .build()
            .expect("writability script");

        let mut cmd = Command::new(runtime);
        cmd.arg("run").arg("--rm");
        // Run as the image's default runtime user (e.g., 'coder') so that HOME subtree
        // writability reflects the intended container execution environment.
        cmd.arg(image).arg("sh").arg("-lc").arg(&script);

        assert!(!script.contains('\0'), "script must not contain NUL");

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
