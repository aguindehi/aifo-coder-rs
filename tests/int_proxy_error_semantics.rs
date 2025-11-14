mod support;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
#[test]
fn int_error_semantics_tcp_v1_and_v2() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Helper: parse port from http://host.docker.internal:<port>/exec
    fn port_from_url(url: &str) -> u16 {
        support::port_from_http_url(url)
    }

    // Helper moved to tests/support::http_post_tcp

    // 1) 401 Unauthorized (no Authorization header)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        let sid = "ut-err-401";
        let (_url, _token, flag, handle) =
            aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
        let port = port_from_url(&_url);
        let (status, headers, body) =
            support::http_post_tcp(port, &[], &[("tool", "cargo"), ("cwd", ".")]);
        assert_eq!(
            status, 401,
            "expected 401, got {status}\nheaders:\n{headers}"
        );
        assert!(
            headers.to_ascii_lowercase().contains("x-exit-code: 86"),
            "expected X-Exit-Code: 86 in headers:\n{}",
            headers
        );
        assert_eq!(String::from_utf8_lossy(&body), "unauthorized\n");
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
    }

    // 2) 426 Upgrade Required (Authorization valid but missing/unsupported X-Aifo-Proto)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        let sid = "ut-err-426";
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
        let port = port_from_url(&url);
        // Send Authorization but no X-Aifo-Proto
        let (status, headers, body) = support::http_post_tcp(
            port,
            &[("Authorization", &format!("Bearer {}", token))],
            &[("tool", "cargo"), ("cwd", ".")],
        );
        assert_eq!(
            status, 426,
            "expected 426, got {status}\nheaders:\n{headers}"
        );
        assert!(
            String::from_utf8_lossy(&body).contains("expected 1 or 2"),
            "expected body to mention expected 1 or 2, got:\n{}",
            String::from_utf8_lossy(&body)
        );
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
    }

    // 3) 403 Forbidden (tool not allowed)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        // Start rust sidecar (to pass routing), then proxy
        let runtime = match aifo_coder::container_runtime_path() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skipping: docker not found in PATH");
                return;
            }
        };
        let ok = std::process::Command::new(&runtime)
            .arg("ps")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("skipping: Docker daemon not reachable");
            return;
        }
        let kinds = vec!["rust".to_string()];
        let overrides: Vec<(String, String)> = Vec::new();
        let no_cache = false;
        let verbose = true;
        let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
            .expect("start rust sidecar");
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");
        let port = port_from_url(&url);
        // Request "ls" which is not present in allowlists
        let (status, headers, body) = support::http_post_tcp(
            port,
            &[
                ("Authorization", &format!("Bearer {}", token)),
                ("X-Aifo-Proto", "1"),
            ],
            &[("tool", "ls"), ("cwd", ".")],
        );
        assert_eq!(
            status, 403,
            "expected 403, got {status}\nheaders:\n{headers}"
        );
        assert!(
            headers.to_ascii_lowercase().contains("x-exit-code: 86"),
            "expected X-Exit-Code: 86 in headers:\n{}",
            headers
        );
        assert_eq!(String::from_utf8_lossy(&body), "forbidden\n");
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, verbose);
    }

    // 4) 409 Conflict (dev-tool not available in any running sidecar)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        let sid = "ut-err-409";
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(sid, true).expect("start proxy");
        let port = port_from_url(&url);
        let (status, _headers, body) = support::http_post_tcp(
            port,
            &[
                ("Authorization", &format!("Bearer {}", token)),
                ("X-Aifo-Proto", "1"),
            ],
            &[("tool", "make"), ("cwd", ".")],
        );
        assert_eq!(status, 409, "expected 409, got {status}");
        let body_s = String::from_utf8_lossy(&body);
        assert!(
            body_s.contains("tool 'make' not available")
                || body_s.contains("start an appropriate toolchain"),
            "expected helpful guidance in 409 body, got:\n{}",
            body_s
        );
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
    }

    // 5) 504 Gateway Timeout (child execution exceeded timeout)
    {
        std::env::remove_var("AIFO_TOOLEEXEC_USE_UNIX");
        std::env::set_var("AIFO_TOOLEEXEC_TIMEOUT_SECS", "1");

        // Ensure Docker daemon is reachable before starting sidecar
        let runtime = match aifo_coder::container_runtime_path() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skipping: docker not found in PATH");
                return;
            }
        };
        let ok = std::process::Command::new(&runtime)
            .arg("ps")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            eprintln!("skipping: Docker daemon not reachable");
            return;
        }

        let kinds = vec!["node".to_string()];
        let overrides: Vec<(String, String)> = Vec::new();
        let no_cache = false;
        let verbose = true;
        let sid = aifo_coder::toolchain_start_session(&kinds, &overrides, no_cache, verbose)
            .expect("start node sidecar");
        let (url, token, flag, handle) =
            aifo_coder::toolexec_start_proxy(&sid, true).expect("start proxy");
        let port = port_from_url(&url);
        // Use protocol v1 (buffered) to trigger recv_timeout. Run node for ~2 seconds.
        let (status, headers, _body) = support::http_post_tcp(
            port,
            &[
                ("Authorization", &format!("Bearer {}", token)),
                ("X-Aifo-Proto", "1"),
            ],
            &[
                ("tool", "node"),
                ("cwd", "."),
                ("arg", "-e"),
                ("arg", "setTimeout(()=>{},2000)"),
            ],
        );
        assert_eq!(
            status, 504,
            "expected 504, got {status}\nheaders:\n{headers}"
        );
        assert!(
            headers.to_ascii_lowercase().contains("x-exit-code: 124"),
            "expected X-Exit-Code: 124 in headers:\n{}",
            headers
        );
        flag.store(false, std::sync::atomic::Ordering::SeqCst);
        let _ = handle.join();
        aifo_coder::toolchain_cleanup_session(&sid, verbose);
        std::env::remove_var("AIFO_TOOLEEXEC_TIMEOUT_SECS");
    }
}
