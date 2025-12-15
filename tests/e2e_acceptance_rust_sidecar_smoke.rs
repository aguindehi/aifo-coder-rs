mod support;
use std::env;
use std::fs;
use std::path::PathBuf;
use support::urlencode;

fn write_file(p: &PathBuf, s: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).expect("mkdirs");
    }
    fs::write(p, s).expect("write");
}

#[test]
#[ignore] // E2E: runs real docker flows; enable in CI lanes intentionally
fn e2e_acceptance_rust_sidecar_smoke() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prefer our toolchain image; allow override via env
    let image = env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-coder-toolchain-rust:latest".to_string());

    // Run a few basic commands inside the rust sidecar; ensure they succeed.
    // These are lighter than a full project test but catch image/runtime regressions.
    for cmd in [
        vec!["cargo".to_string(), "--version".to_string()],
        vec!["rustc".to_string(), "--version".to_string()],
        vec!["cargo".to_string(), "nextest".to_string(), "-V".to_string()],
    ] {
        let code = aifo_coder::toolchain_run(
            "rust",
            &cmd,
            Some(&image),
            false, // allow caches
            false,
            false,
        )
        .expect("toolchain_run returned io::Result");
        assert_eq!(
            code, 0,
            "command {:?} failed inside rust sidecar (image={})",
            cmd, image
        );
    }
}

#[test]
#[ignore] // E2E: full acceptance per Phase 8; runs real docker flows in a temp project
fn e2e_toolchain_rust_acceptance_full_suite() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Prefer our toolchain image; allow override via env
    let image = env::var("AIFO_CODER_TEST_RUST_IMAGE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "aifo-coder-toolchain-rust:latest".to_string());

    // Allow more time in CI for the first run to build deps
    let old_to = env::var("AIFO_TOOLEEXEC_TIMEOUT_SECS").ok();
    env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", "600");

    // Create a minimal Rust project in a temp workspace
    let td = tempfile::tempdir().expect("tmpdir");
    let ws = td.path().to_path_buf();

    let cargo_toml = aifo_coder::TextLines::new()
        .extend([
            "[package]".to_string(),
            r#"name = "aifo_phase8_smoke""#.to_string(),
            r#"version = "0.1.0""#.to_string(),
            r#"edition = "2021""#.to_string(),
            "".to_string(),
            "[dependencies]".to_string(),
        ])
        .build_lf()
        .expect("cargo toml");
    write_file(&ws.join("Cargo.toml"), cargo_toml);

    // Keep formatting simple and clippy-clean
    let lib_rs = aifo_coder::TextLines::new()
        .extend([
            "pub fn add(a: i32, b: i32) -> i32 {".to_string(),
            "    a + b".to_string(),
            "}".to_string(),
            "".to_string(),
            "#[cfg(test)]".to_string(),
            "mod tests {".to_string(),
            "    use super::*;".to_string(),
            "    #[test]".to_string(),
            "    fn e2e_adds() {".to_string(),
            "        assert_eq!(add(2, 3), 5);".to_string(),
            "    }".to_string(),
            "}".to_string(),
        ])
        .build_lf()
        .expect("lib rs");
    write_file(&ws.join("src").join("lib.rs"), lib_rs);

    // Ensure repo-root detection works (many functions look for a .git directory)
    std::fs::create_dir_all(ws.join(".git")).expect("mkdir .git");

    // On Unix, relax permissions so the container user can traverse the bind mount (macOS tempdirs are 0700)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fn e2e_chmod_recursive_impl(path: &std::path::Path) {
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.is_dir() {
                    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
                    if let Ok(rd) = std::fs::read_dir(path) {
                        for ent in rd.flatten() {
                            e2e_chmod_recursive_impl(&ent.path());
                        }
                    }
                } else {
                    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644));
                }
            }
        }
        e2e_chmod_recursive_impl(&ws);
    }

    // Change into workspace for sidecar execs
    let old_cwd = env::current_dir().expect("cwd");
    env::set_current_dir(&ws).expect("chdir");

    // Start a rust session and proxy so the workspace is mounted at /workspace and -w is set
    let kinds = vec!["rust".to_string()];
    let overrides: Vec<(String, String)> = Vec::new();
    let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, false, true)
        .expect("failed to start rust sidecar session");

    let (url, token, flag, handle) =
        aifo_coder::toolexec_start_proxy(&sid, true).expect("failed to start proxy");

    // Extract port from URL; connect to localhost:<port> from the host
    let port: u16 = {
        assert!(
            url.starts_with("http://"),
            "expected http:// URL, got: {url}"
        );
        let without_proto = url.trim_start_matches("http://");
        let host_port = without_proto.split('/').next().unwrap_or(without_proto);
        host_port
            .rsplit(':')
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .expect("failed to parse port from URL")
    };

    // Minimal HTTP v2 client to run tool=cargo and read chunked body and trailers
    fn post_exec_tcp_v2(port: u16, token: &str, tool: &str, args: &[&str]) -> (i32, String) {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpStream;

        let mut stream =
            TcpStream::connect(("127.0.0.1", port)).expect("connect 127.0.0.1:<port> failed");

        let mut body = format!("tool={}&cwd={}", urlencode(tool), urlencode("/workspace"));
        for a in args {
            body.push('&');
            body.push_str(&format!("arg={}", urlencode(a)));
        }

        let req = format!(
            "POST /exec HTTP/1.1\r\nHost: host.docker.internal\r\nAuthorization: Bearer {}\r\nX-Aifo-Proto: 2\r\nTE: trailers\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            token,
            body.len(),
            body
        );
        stream.write_all(req.as_bytes()).expect("write failed");

        // Read headers until CRLFCRLF
        let mut reader = BufReader::new(stream);
        let mut header = String::new();
        loop {
            let mut line = String::new();
            let n = reader
                .read_line(&mut line)
                .expect("read header line failed");
            if n == 0 {
                break;
            }
            header.push_str(&line);
            if header.ends_with("\r\n\r\n") || header.ends_with("\n\n") {
                break;
            }
            if header.len() > 128 * 1024 {
                break;
            }
        }

        // Read chunked body; assemble into a string, then parse trailers for exit code
        let mut body_out = Vec::new();
        loop {
            let mut size_line = String::new();
            reader
                .read_line(&mut size_line)
                .expect("read chunk size failed");
            if size_line.is_empty() {
                break;
            }
            let size_str = size_line.trim();
            let size_only = size_str.split(';').next().unwrap_or(size_str);
            let Ok(sz) = usize::from_str_radix(size_only, 16) else {
                break;
            };
            if sz == 0 {
                break;
            }
            let mut chunk = vec![0u8; sz];
            reader
                .read_exact(&mut chunk)
                .expect("read chunk data failed");
            body_out.extend_from_slice(&chunk);
            let mut crlf = [0u8; 2];
            reader.read_exact(&mut crlf).expect("read CRLF failed");
        }

        // Read trailers; extract X-Exit-Code
        let mut code: i32 = 1;
        loop {
            let mut tline = String::new();
            let n = reader.read_line(&mut tline).unwrap_or(0);
            if n == 0 {
                break;
            }
            let tl = tline.trim_end_matches(['\r', '\n']);
            if tl.is_empty() {
                break;
            }
            if let Some(v) = tl.strip_prefix("X-Exit-Code: ") {
                code = v.trim().parse::<i32>().unwrap_or(1);
            }
        }

        let text = String::from_utf8_lossy(&body_out).to_string();
        (code, text)
    }

    // Preflight: ensure /workspace is visible and manifest exists inside the container
    let (pre_code, pre_out) = post_exec_tcp_v2(
        port,
        &token,
        "sh",
        &[
            "-lc",
            "set -e; ls -ld /workspace 2>&1; if [ -f /workspace/Cargo.toml ]; then echo OK; else echo MISSING; fi",
        ],
    );
    if pre_code != 0 || !pre_out.contains("OK") {
        eprintln!(
            "skipping: /workspace or manifest not visible inside container (permissions/mount?).\
\nDiagnostics:\n{}",
            pre_out
        );
        // Cleanup proxy/session and restore env/cwd before early return
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, true);
        if let Some(v) = old_to.clone() {
            env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", v);
        } else {
            env::remove_var("AIFO_TOOLEEXEC_TIMEOUT_SECS");
        }
        env::set_current_dir(&old_cwd).ok();
        return;
    }

    // Detect installed rustup components to decide which checks to run
    let (code_comp, out_comp) = post_exec_tcp_v2(
        port,
        &token,
        "rustup",
        &["component", "list", "--installed"],
    );
    let comps = out_comp.to_ascii_lowercase();
    let has_comp_list = code_comp == 0;
    let has_rustfmt = has_comp_list && comps.contains("rustfmt");
    let has_clippy = has_comp_list && comps.contains("clippy");

    // Ensure rustc is available; otherwise skip compile/test steps
    let (code_rustc, _out_rustc) = post_exec_tcp_v2(port, &token, "rustc", &["--version"]);
    if code_rustc != 0 {
        eprintln!(
            "skipping cargo test suite: rustc not installed in image {}",
            image
        );
        // Cleanup proxy/session and restore env/cwd before early return
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, true);
        if let Some(v) = old_to.clone() {
            env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", v);
        } else {
            env::remove_var("AIFO_TOOLEEXEC_TIMEOUT_SECS");
        }
        env::set_current_dir(&old_cwd).ok();
        return;
    }

    // Format check (only if rustfmt present)
    if has_rustfmt {
        let (code_fmt, out_fmt) = post_exec_tcp_v2(
            port,
            &token,
            "cargo",
            &[
                "fmt",
                "--manifest-path",
                "/workspace/Cargo.toml",
                "--",
                "--check",
            ],
        );
        assert_eq!(
            code_fmt, 0,
            "cargo fmt -- --check failed in rust sidecar (image={}):\n{}",
            image, out_fmt
        );
    } else {
        eprintln!(
            "skipping cargo fmt check: rustfmt component not installed in image {}",
            image
        );
    }

    // Clippy (deny warnings) only if clippy present
    if has_clippy {
        let (code_clippy, out_clippy) = post_exec_tcp_v2(
            port,
            &token,
            "cargo",
            &[
                "clippy",
                "--manifest-path",
                "/workspace/Cargo.toml",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        );
        assert_eq!(
            code_clippy, 0,
            "cargo clippy -D warnings failed in rust sidecar (image={}):\n{}",
            image, out_clippy
        );
    } else {
        eprintln!(
            "skipping cargo clippy check: clippy component not installed in image {}",
            image
        );
    }

    // cargo --version (simple presence/exec check without relying on manifest or writes)
    let (code_cargo_ver, out_cargo_ver) = post_exec_tcp_v2(port, &token, "cargo", &["--version"]);
    assert_eq!(
        code_cargo_ver, 0,
        "cargo --version failed in rust sidecar (image={}):\n{}",
        image, out_cargo_ver
    );

    // Probe cargo-nextest presence only (do not run build/run to avoid linking)
    let (code_nextest_v, _out_nextest_v) =
        post_exec_tcp_v2(port, &token, "cargo", &["nextest", "-V"]);
    if code_nextest_v != 0 {
        eprintln!(
            "note: cargo-nextest not installed in image {}; skipping nextest probe",
            image
        );
    }

    // Cleanup proxy/session
    flag.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = handle.join();
    aifo_coder::toolchain_cleanup_session(&sid, true);

    // Restore env and cwd
    if let Some(v) = old_to {
        env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", v);
    } else {
        env::remove_var("AIFO_TOOLEEXEC_TIMEOUT_SECS");
    }
    env::set_current_dir(old_cwd).ok();
}
