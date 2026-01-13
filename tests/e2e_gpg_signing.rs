use std::env;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::tempdir;
use walkdir::WalkDir;

fn docker_available() -> bool {
    aifo_coder::container_runtime_path().is_ok()
}

fn host_gpg_available() -> bool {
    Command::new("gpg")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn image_present(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in WalkDir::new(src) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(src).unwrap();
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn seed_gpg(root: &Path, passphrase: &str) -> std::io::Result<PathBuf> {
    let gnupg = root.join("seed-gnupg");
    fs::create_dir_all(&gnupg)?;
    fs::write(
        gnupg.join("gpg-agent.conf"),
        b"allow-loopback-pinentry\nallow-preset-passphrase\npinentry-program /usr/bin/pinentry-curses\n",
    )?;
    fs::write(gnupg.join("gpg.conf"), b"pinentry-mode loopback\n")?;

    let status = Command::new("gpg")
        .env("GNUPGHOME", &gnupg)
        .args([
            "--batch",
            "--yes",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            passphrase,
            "--quick-generate-key",
            "Test User <test@mgb.ch>",
            "ed25519",
            "sign",
            "0",
        ])
        .status()?;
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "failed to seed gpg key",
        ));
    }
    Ok(gnupg)
}

fn fullscreen(agent: &str) -> bool {
    matches!(
        agent,
        "opencode" | "opencode-slim" | "codex" | "codex-slim" | "crush" | "crush-slim"
    )
}

#[ignore]
#[test]
fn e2e_gpg_signing_across_agents() {
    if !docker_available() {
        eprintln!("skipping: docker not available");
        return;
    }
    if !host_gpg_available() {
        eprintln!("skipping: host gpg not available to seed test key");
        return;
    }

    let tmp = tempdir().expect("tmpdir");
    let root = tmp.path();

    // Use a copied docker config to avoid mutating the caller's config.
    let docker_cfg_src = env::var("DOCKER_CONFIG")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home::home_dir().map(|h| {
                let candidate = h.join(".docker");
                if candidate.exists() {
                    candidate
                } else {
                    PathBuf::new()
                }
            })
        })
        .filter(|p| p.exists());
    let docker_cfg_dst = root.join("docker-config");
    if let Some(src) = docker_cfg_src {
        copy_dir(&src, &docker_cfg_dst).expect("copy docker config");
    } else {
        fs::create_dir_all(&docker_cfg_dst).expect("docker config dst");
    }

    let passphrase = "test-passphrase";
    let seed_dir = seed_gpg(root, passphrase).expect("seed gpg");
    #[cfg(unix)]
    let _ = std::fs::set_permissions(&seed_dir, std::fs::Permissions::from_mode(0o700));

    let agents = vec![
        "opencode",
        "codex",
        "crush",
        "aider",
        "openhands",
        "plandex",
    ];
    let image_prefix =
        env::var("AIFO_TEST_IMAGE_PREFIX").unwrap_or_else(|_| "aifo-coder".to_string());
    let image_tag = env::var("AIFO_TEST_IMAGE_TAG").unwrap_or_else(|_| "latest".to_string());

    let script = r#"
set -euo pipefail
home="${HOME:-/home/coder}"
gnupg="${GNUPGHOME:-$home/.gnupg}"
pass="${AIFO_GPG_PASSPHRASE:-test-passphrase}"
mkdir -p "$home" "$gnupg" "${XDG_RUNTIME_DIR:-/tmp/runtime-$$}"
chmod 700 "$home" "$gnupg" "${XDG_RUNTIME_DIR:-/tmp/runtime-$$}" || true

for b in gpg gpg-agent pinentry-curses git; do
  command -v "$b" >/dev/null 2>&1 || { echo "missing binary: $b"; exit 1; }
done

preset_bin=""
for p in gpg-preset-passphrase /usr/lib/gnupg/gpg-preset-passphrase /usr/lib/gnupg2/gpg-preset-passphrase; do
  if command -v "$p" >/dev/null 2>&1; then preset_bin="$(command -v "$p")"; break; fi
  if [ -x "$p" ]; then preset_bin="$p"; break; fi
done
[ -n "$preset_bin" ] || { echo "missing binary: gpg-preset-passphrase"; exit 1; }

grep -q '^allow-loopback-pinentry' "$gnupg/gpg-agent.conf" || { echo "missing allow-loopback-pinentry"; exit 1; }
grep -q '^allow-preset-passphrase' "$gnupg/gpg-agent.conf" || { echo "missing allow-preset-passphrase"; exit 1; }
grep -q '^pinentry-program' "$gnupg/gpg-agent.conf" || { echo "missing pinentry-program"; exit 1; }
case "${AIFO_AGENT_NAME:-}" in
  opencode|opencode-slim|codex|codex-slim|crush|crush-slim)
    grep -q '^pinentry-mode loopback' "$gnupg/gpg.conf" || { echo "missing pinentry-mode loopback"; exit 1; }
    ;;
esac

[ -S "$gnupg/S.gpg-agent" ] || gpgconf --launch gpg-agent >/dev/null 2>&1 || true

fpr="$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1=="fpr"{print $10; exit}')"
grip="$(gpg --batch --with-colons --with-keygrip --list-secret-keys | awk -F: '$1=="grp"{print $10; exit}')"
[ -n "$fpr" ] && [ -n "$grip" ] || { echo "no secret key available"; exit 1; }

printf '%s\n' "$pass" | "$preset_bin" --homedir "$gnupg" --preset "$grip" >/dev/null 2>&1 || { echo "gpg-preset-passphrase failed"; exit 1; }

cached="$(gpg-connect-agent "keyinfo $grip" /bye | awk '/^S KEYINFO/{print $0}')"
echo "$cached" | grep -q ' 1 P' || { echo "passphrase not cached: $cached"; exit 1; }

echo test | gpg --batch --yes --pinentry-mode loopback --local-user "$fpr" --clearsign >/tmp/sig.txt

git init -q
echo hello > file.txt
git add file.txt
git -c user.name="Test User" -c user.email="test@mgb.ch" -c user.signingkey="$fpr" -c commit.gpgsign=true commit -qm "signed commit"
"#;

    for agent in agents {
        let image = format!("{}-{}:{}", image_prefix, agent, image_tag);
        if !image_present(&image) {
            eprintln!("skipping: image not present locally: {}", image);
            continue;
        }

        // Per-agent GNUPGHOME (copy seeded key) to avoid cross-agent socket reuse.
        let agent_seed = root.join(format!("seed-{}", agent));
        fs::create_dir_all(&agent_seed).expect("agent seed dir");
        copy_dir(&seed_dir, &agent_seed).expect("copy seed gnupg");
        // Ensure perms are acceptable to gpg inside the container.
        #[cfg(unix)]
        let _ = std::fs::set_permissions(&agent_seed, std::fs::Permissions::from_mode(0o700));

        let mut cmd = Command::new("docker");
        cmd.env("DOCKER_CONFIG", &docker_cfg_dst);
        cmd.args([
            "run",
            "--rm",
            "-i",
            "-e",
            "HOME=/home/coder",
            "-e",
            "AIFO_RUNTIME_USER=root",
            "-e",
            "GNUPGHOME=/home/coder/.gnupg",
            "-e",
            "XDG_RUNTIME_DIR=/tmp/runtime-1000",
            "-e",
            &format!("AIFO_AGENT_NAME={agent}"),
            "-e",
            &format!("AIFO_GPG_PASSPHRASE={passphrase}"),
        ]);
        if fullscreen(agent) {
            cmd.args(["-e", "AIFO_GPG_REQUIRE_PRIME=1"]);
        }
        cmd.args([
            "-v",
            &format!("{}:/home/coder/.gnupg-host", agent_seed.display()),
            &image,
            "sh",
            "-lc",
            script,
        ]);

        let output = cmd.output().expect("docker run");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "docker run failed for agent={}: status={:?}\nstdout:\n{}\nstderr:\n{}",
            agent,
            output.status.code(),
            stdout,
            stderr
        );
    }
}
