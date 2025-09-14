use clap::ValueEnum;
use std::io::{Read, Write};
use std::time::{Duration, SystemTime};
mod apparmor;
mod color;
mod docker;
mod fork;
mod lock;
mod registry;
mod toolchain;
mod util;
mod ui;
pub use apparmor::*;
pub use color::*;
pub use docker::*;
pub use fork::*;
pub use lock::*;
pub use registry::*;
pub use toolchain::*;
pub use util::*;
pub use ui::warn::{warn_print, warn_prompt_continue_or_quit};

#[cfg(windows)]
fn ps_quote_inner(s: &str) -> String {
    let esc = s.replace('\'', "''");
    format!("'{}'", esc)
}

#[cfg(windows)]
/// Build the PowerShell inner command for fork panes (used by tests).
pub fn fork_ps_inner_string(
    agent: &str,
    sid: &str,
    i: usize,
    pane_dir: &std::path::Path,
    pane_state_dir: &std::path::Path,
    child_args: &[String],
) -> String {
    let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
    let kv = [
        ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
        ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
        ("AIFO_CODER_HOSTNAME", cname),
        ("AIFO_CODER_FORK_SESSION", sid.to_string()),
        ("AIFO_CODER_FORK_INDEX", i.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR",
            pane_state_dir.display().to_string(),
        ),
    ];
    let mut assigns: Vec<String> = Vec::new();
    for (k, v) in kv {
        assigns.push(format!("$env:{}={}", k, ps_quote_inner(&v)));
    }
    let mut words: Vec<String> = vec!["aifo-coder".to_string()];
    words.extend(child_args.iter().cloned());
    let cmd = words
        .iter()
        .map(|w| ps_quote_inner(w))
        .collect::<Vec<_>>()
        .join(" ");
    let setloc = format!(
        "Set-Location {}",
        ps_quote_inner(&pane_dir.display().to_string())
    );
    format!("{}; {}; {}", setloc, assigns.join("; "), cmd)
}

#[cfg(windows)]
/// Build the Git Bash inner command for fork panes (used by tests).
pub fn fork_bash_inner_string(
    agent: &str,
    sid: &str,
    i: usize,
    pane_dir: &std::path::Path,
    pane_state_dir: &std::path::Path,
    child_args: &[String],
) -> String {
    let cname = format!("aifo-coder-{}-{}-{}", agent, sid, i);
    let kv = [
        ("AIFO_CODER_SKIP_LOCK", "1".to_string()),
        ("AIFO_CODER_CONTAINER_NAME", cname.clone()),
        ("AIFO_CODER_HOSTNAME", cname),
        ("AIFO_CODER_FORK_SESSION", sid.to_string()),
        ("AIFO_CODER_FORK_INDEX", i.to_string()),
        (
            "AIFO_CODER_FORK_STATE_DIR",
            pane_state_dir.display().to_string(),
        ),
    ];
    let mut exports: Vec<String> = Vec::new();
    for (k, v) in kv {
        exports.push(format!("export {}={}", k, shell_escape(&v)));
    }
    let mut words: Vec<String> = vec!["aifo-coder".to_string()];
    words.extend(child_args.iter().cloned());
    let cmd = shell_join(&words);
    let cddir = shell_escape(&pane_dir.display().to_string());
    format!("cd {} && {}; {}; exec bash", cddir, exports.join("; "), cmd)
}

#[cfg(windows)]
/// Map layout to wt.exe split orientation flag.
pub fn wt_orient_for_layout(layout: &str, i: usize) -> &'static str {
    match layout {
        "even-h" => "-H",
        "even-v" => "-V",
        _ => {
            if i % 2 == 0 {
                "-H"
            } else {
                "-V"
            }
        }
    }
}

#[cfg(windows)]
/// Build argument vector for `wt new-tab -d <dir> <psbin> -NoExit -Command <inner>`.
pub fn wt_build_new_tab_args(
    psbin: &std::path::Path,
    pane_dir: &std::path::Path,
    inner: &str,
) -> Vec<String> {
    vec![
        "wt".to_string(),
        "new-tab".to_string(),
        "-d".to_string(),
        pane_dir.display().to_string(),
        psbin.display().to_string(),
        "-NoExit".to_string(),
        "-Command".to_string(),
        inner.to_string(),
    ]
}

#[cfg(windows)]
/// Build argument vector for `wt split-pane <orient> -d <dir> <psbin> -NoExit -Command <inner>`.
pub fn wt_build_split_args(
    orient: &str,
    psbin: &std::path::Path,
    pane_dir: &std::path::Path,
    inner: &str,
) -> Vec<String> {
    vec![
        "wt".to_string(),
        "split-pane".to_string(),
        orient.to_string(),
        "-d".to_string(),
        pane_dir.display().to_string(),
        psbin.display().to_string(),
        "-NoExit".to_string(),
        "-Command".to_string(),
        inner.to_string(),
    ]
}

#[cfg(windows)]
/// Build a PowerShell Wait-Process command from a list of PIDs.
pub fn ps_wait_process_cmd(ids: &[&str]) -> String {
    format!("Wait-Process -Id {}", ids.join(","))
}

// -------- Color mode and helpers --------



/**
 Merging strategy for post-fork actions.
 - None: do nothing (default).
 - Fetch: fetch pane branches back into the original repository as local branches.
 - Octopus: fetch branches then attempt an octopus merge into a merge/<sid> branch.
*/
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
pub enum MergingStrategy {
    #[value(name = "none")]
    None,
    #[value(name = "fetch")]
    Fetch,
    #[value(name = "octopus")]
    Octopus,
}

/// Render a docker -v host:container pair.
pub fn path_pair(host: &std::path::Path, container: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(format!("{}:{container}", host.display()))
}

/// Ensure a file exists by creating parent directories as needed.
pub fn ensure_file_exists(p: &std::path::Path) -> std::io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(p)?;
    }
    Ok(())
}

/// Repository-scoped locking helpers and candidate paths
pub fn create_session_id() -> String {
    // Compose a short, mostly-unique ID from time and pid without extra deps
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0));
    let pid = std::process::id() as u128;
    let nanos = now.as_nanos();
    let mix = nanos ^ (pid as u128);
    // base36 encode last 40 bits for brevity
    let mut v = (mix & 0xffffffffff) as u64;
    let mut s = String::new();
    let alphabet = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if v == 0 {
        s.push('0');
    } else {
        while v > 0 {
            let idx = (v % 36) as usize;
            s.push(alphabet[idx] as char);
            v /= 36;
        }
    }
    s.chars().rev().collect()
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate as aifo_coder;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    // Serialize tests that mutate HOME/AIFO_NOTIFICATIONS_CONFIG to avoid env races
    static NOTIF_ENV_GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn test_url_decode_mixed() {
        assert_eq!(url_decode("a+b%20c%2F%3F%25"), "a b c/?%");
        assert_eq!(url_decode("%41%42%43"), "ABC");
        assert_eq!(url_decode("no-escapes_here~"), "no-escapes_here~");
    }

    #[test]
    fn test_parse_form_urlencoded_basic_and_repeated() {
        let pairs = parse_form_urlencoded("arg=a&arg=b&tool=cargo&cwd=.");
        let expected = vec![
            ("arg".to_string(), "a".to_string()),
            ("arg".to_string(), "b".to_string()),
            ("tool".to_string(), "cargo".to_string()),
            ("cwd".to_string(), ".".to_string()),
        ];
        assert_eq!(pairs, expected);
    }

    #[test]
    fn test_find_crlfcrlf_cases() {
        assert_eq!(find_crlfcrlf(b"\r\n\r\n"), Some(0));
        assert_eq!(find_crlfcrlf(b"abc\r\n\r\ndef"), Some(3));
        assert_eq!(find_crlfcrlf(b"abcdef"), None);
        assert_eq!(find_crlfcrlf(b"\r\n\r"), None);
    }

    #[test]
    fn test_strip_outer_quotes_variants() {
        assert_eq!(strip_outer_quotes("'abc'"), "abc");
        assert_eq!(strip_outer_quotes("\"abc\""), "abc");
        assert_eq!(strip_outer_quotes("'a b'"), "a b");
        assert_eq!(strip_outer_quotes("noquote"), "noquote");
        // Only strips if both ends match the same quote type
        assert_eq!(strip_outer_quotes("'mismatch\""), "'mismatch\"");
    }

    #[test]
    fn test_shell_like_split_args_quotes_and_spaces() {
        let args = shell_like_split_args("'a b' c \"d e\"");
        assert_eq!(
            args,
            vec!["a b".to_string(), "c".to_string(), "d e".to_string()]
        );

        let args2 = shell_like_split_args("  a   'b c'   d  ");
        assert_eq!(
            args2,
            vec!["a".to_string(), "b c".to_string(), "d".to_string()]
        );
    }

    #[test]
    fn test_parse_notifications_inline_array() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with an inline-array notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: ["say", "--title", "AIFO"]\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        // Force parser to use this exact file path to avoid HOME/env races
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications array");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }

        // Restore HOME
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_parse_notifications_nested_array_lines() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a nested array notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command:
  - "say"
  - --title
  - AIFO
"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications nested array");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore env
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_parse_notifications_block_scalar() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a block scalar notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: |
  say --title "AIFO"
"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications block");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore env
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_parse_notifications_single_line_string() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a single-line string notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: "say --title AIFO"\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        // Force parser to use this exact file path to avoid HOME/env races
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let argv = parse_notifications_command_config().expect("parse notifications string");
        assert_eq!(
            argv,
            vec!["say".to_string(), "--title".to_string(), "AIFO".to_string()]
        );
        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }

        // Restore HOME
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_build_sidecar_exec_preview_python_venv_env() {
        // Create a temp workspace with .venv/bin and ensure PATH/VIRTUAL_ENV are injected
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        std::fs::create_dir_all(pwd.join(".venv").join("bin")).expect("create venv/bin");
        let user_args = vec!["python".to_string(), "--version".to_string()];
        let args = build_sidecar_exec_preview("tc-python", None, pwd, "python", &user_args);

        let has_virtual_env = args.iter().any(|s| s == "VIRTUAL_ENV=/workspace/.venv");
        let has_path_prefix = args
            .iter()
            .any(|s| s.contains("PATH=/workspace/.venv/bin:"));
        assert!(
            has_virtual_env,
            "exec preview missing VIRTUAL_ENV: {:?}",
            args
        );
        assert!(
            has_path_prefix,
            "exec preview missing PATH venv prefix: {:?}",
            args
        );
    }

    #[test]
    fn test_notifications_config_rejects_non_say() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Isolate HOME to a temp dir with a non-say notifications-command
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: ["notify", "--title", "AIFO"]\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        // Force parser to use this exact file path to avoid platform-specific HOME quirks
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        let res = notifications_handle_request(&["--title".into(), "AIFO".into()], false, 1);
        assert!(res.is_err(), "expected error when executable is not 'say'");
        let msg = res.err().unwrap();
        assert!(
            msg.contains("only 'say' is allowed"),
            "unexpected error: {}",
            msg
        );

        // Restore AIFO_NOTIFICATIONS_CONFIG
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }

        // Restore HOME
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_sidecar_run_preview_add_host_flag_linux() {
        // Ensure add-host is injected for sidecars when env flag is set
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let old = std::env::var("AIFO_TOOLEEXEC_ADD_HOST").ok();
        std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", "1");

        let args = build_sidecar_run_preview(
            "tc-rust-test",
            Some("aifo-net-test"),
            None,
            "rust",
            "rust:1.80-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let joined = shell_join(&args);
        assert!(
            joined.contains("--add-host host.docker.internal:host-gateway"),
            "sidecar run preview missing --add-host: {}",
            joined
        );

        // Restore env
        if let Some(v) = old {
            std::env::set_var("AIFO_TOOLEEXEC_ADD_HOST", v);
        } else {
            std::env::remove_var("AIFO_TOOLEEXEC_ADD_HOST");
        }
    }

    #[test]
    fn test_sidecar_run_preview_rust_caches_env() {
        // Ensure rust sidecar gets cargo cache mounts and CARGO_HOME
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let args = build_sidecar_run_preview(
            "tc-rust-cache",
            Some("aifo-net-x"),
            None,
            "rust",
            "rust:1.80-slim",
            false, // no_cache = false -> caches enabled
            pwd,
            Some("docker-default"),
        );
        let joined = shell_join(&args);
        assert!(
            joined.contains("aifo-cargo-registry:/usr/local/cargo/registry"),
            "missing cargo registry mount: {}",
            joined
        );
        assert!(
            joined.contains("aifo-cargo-git:/usr/local/cargo/git"),
            "missing cargo git mount: {}",
            joined
        );
        assert!(
            joined.contains("CARGO_HOME=/home/coder/.cargo"),
            "missing CARGO_HOME env: {}",
            joined
        );
    }

    #[test]
    fn test_sidecar_run_preview_caches_for_node_python_cpp_go() {
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();

        // node: npm cache
        let node = build_sidecar_run_preview(
            "tc-node-cache",
            Some("aifo-net-x"),
            None,
            "node",
            "node:20-bookworm-slim",
            false,
            pwd,
            Some("docker-default"),
        );
        let node_joined = shell_join(&node);
        assert!(
            node_joined.contains("aifo-npm-cache:/home/coder/.npm"),
            "missing npm cache mount: {}",
            node_joined
        );

        // python: pip cache
        let py = build_sidecar_run_preview(
            "tc-python-cache",
            Some("aifo-net-x"),
            None,
            "python",
            "python:3.12-slim",
            false,
            pwd,
            Some("docker-default"),
        );
        let py_joined = shell_join(&py);
        assert!(
            py_joined.contains("aifo-pip-cache:/home/coder/.cache/pip"),
            "missing pip cache mount: {}",
            py_joined
        );

        // c-cpp: ccache dir and env
        let cpp = build_sidecar_run_preview(
            "tc-cpp-cache",
            Some("aifo-net-x"),
            None,
            "c-cpp",
            "aifo-cpp-toolchain:latest",
            false,
            pwd,
            Some("docker-default"),
        );
        let cpp_joined = shell_join(&cpp);
        assert!(
            cpp_joined.contains("aifo-ccache:/home/coder/.cache/ccache"),
            "missing ccache volume: {}",
            cpp_joined
        );
        assert!(
            cpp_joined.contains("CCACHE_DIR=/home/coder/.cache/ccache"),
            "missing CCACHE_DIR env: {}",
            cpp_joined
        );

        // go: GOPATH/GOMODCACHE/GOCACHE and volume
        let go = build_sidecar_run_preview(
            "tc-go-cache",
            Some("aifo-net-x"),
            None,
            "go",
            "golang:1.22-bookworm",
            false,
            pwd,
            Some("docker-default"),
        );
        let go_joined = shell_join(&go);
        assert!(
            go_joined.contains("aifo-go:/go"),
            "missing go volume: {}",
            go_joined
        );
        assert!(
            go_joined.contains("GOPATH=/go"),
            "missing GOPATH env: {}",
            go_joined
        );
        assert!(
            go_joined.contains("GOMODCACHE=/go/pkg/mod"),
            "missing GOMODCACHE env: {}",
            go_joined
        );
        assert!(
            go_joined.contains("GOCACHE=/go/build-cache"),
            "missing GOCACHE env: {}",
            go_joined
        );
    }

    #[test]
    fn test_build_sidecar_exec_preview_cpp_ccache_env() {
        // c/cpp exec should include CCACHE_DIR env
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let user_args = vec!["cmake".to_string(), "--version".to_string()];
        let args = build_sidecar_exec_preview("tc-cpp", None, pwd, "c-cpp", &user_args);
        let has_ccache = args
            .iter()
            .any(|s| s == "CCACHE_DIR=/home/coder/.cache/ccache");
        assert!(has_ccache, "exec preview missing CCACHE_DIR: {:?}", args);
    }

    #[test]
    fn test_build_sidecar_exec_preview_go_envs() {
        // go exec should include GOPATH/GOMODCACHE/GOCACHE envs
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();
        let user_args = vec!["go".to_string(), "version".to_string()];
        let args = build_sidecar_exec_preview("tc-go", None, pwd, "go", &user_args);
        let has_gopath = args.iter().any(|s| s == "GOPATH=/go");
        let has_mod = args.iter().any(|s| s == "GOMODCACHE=/go/pkg/mod");
        let has_cache = args.iter().any(|s| s == "GOCACHE=/go/build-cache");
        assert!(
            has_gopath && has_mod && has_cache,
            "exec preview missing go envs: {:?}",
            args
        );
    }

    #[test]
    fn test_notifications_args_mismatch_error() {
        let _g = NOTIF_ENV_GUARD.lock().unwrap();
        // Prepare config allowing only ["--title", "AIFO"]
        let td = tempfile::tempdir().expect("tmpdir");
        let home = td.path().to_path_buf();
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);

        let cfg = r#"notifications-command: ["say", "--title", "AIFO"]\n"#;
        let cfg_path = home.join(".aider.conf.yml");
        std::fs::write(&cfg_path, cfg).expect("write config");
        let old_cfg = std::env::var("AIFO_NOTIFICATIONS_CONFIG").ok();
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);

        // Request with mismatching args
        let res = notifications_handle_request(&["--title".into(), "Other".into()], false, 1);
        assert!(res.is_err(), "expected mismatch error, got: {:?}", res);
        let msg = res.err().unwrap();
        assert!(
            msg.contains("arguments mismatch"),
            "unexpected error message: {}",
            msg
        );

        // Restore env
        if let Some(v) = old_cfg {
            std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", v);
        } else {
            std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        }
        if let Some(v) = old_home {
            std::env::set_var("HOME", v);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_candidate_lock_paths_includes_xdg_runtime_dir() {
        // Use a non-repo temp directory to exercise legacy fallback candidates
        let td = tempfile::tempdir().expect("tmpdir");
        let old = std::env::var("XDG_RUNTIME_DIR").ok();
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_var("XDG_RUNTIME_DIR", td.path());
        std::env::set_current_dir(td.path()).expect("chdir");

        let paths = candidate_lock_paths();
        let expected = td.path().join("aifo-coder.lock");
        assert!(
            paths.iter().any(|p| p == &expected),
            "candidate_lock_paths missing expected XDG_RUNTIME_DIR path: {:?}",
            expected
        );

        // Restore env and cwd
        if let Some(v) = old {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        std::env::set_current_dir(old_cwd).ok();
    }

    #[test]
    fn test_candidate_lock_paths_includes_cwd_lock_outside_repo() {
        // In a non-repo directory, ensure CWD/.aifo-coder.lock appears among legacy candidates
        let td = tempfile::tempdir().expect("tmpdir");
        let old_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(td.path()).expect("chdir");
        // Unset repo-related envs to avoid confusing repo detection
        let paths = candidate_lock_paths();
        let expected = td.path().join(".aifo-coder.lock");
        // On macOS, /var is often a symlink to /private/var. Canonicalize parent dirs for comparison.
        let expected_dir_canon =
            std::fs::canonicalize(td.path()).unwrap_or_else(|_| td.path().to_path_buf());
        let found = paths.iter().any(|p| {
            p.file_name()
                .map(|n| n == ".aifo-coder.lock")
                .unwrap_or(false)
                && p.parent()
                    .and_then(|d| std::fs::canonicalize(d).ok())
                    .map(|d| d == expected_dir_canon)
                    .unwrap_or(false)
        });
        assert!(
            found,
            "candidate_lock_paths missing expected CWD lock path: {:?} in {:?}",
            expected, paths
        );
        std::env::set_current_dir(old_cwd).ok();
    }

    #[test]
    fn test_parse_form_urlencoded_empty_and_missing_values() {
        let pairs = parse_form_urlencoded("a=1&b=&c");
        assert!(
            pairs.contains(&(String::from("a"), String::from("1"))),
            "missing a=1 in {:?}",
            pairs
        );
        assert!(
            pairs.contains(&(String::from("b"), String::from(""))),
            "missing b= in {:?}",
            pairs
        );
        assert!(
            pairs.contains(&(String::from("c"), String::from(""))),
            "missing c (no '=') in {:?}",
            pairs
        );
    }

    #[test]
    fn test_sidecar_run_preview_no_cache_removes_cache_mounts() {
        let td = tempfile::tempdir().expect("tmpdir");
        let pwd = td.path();

        // rust: no aifo-cargo-* mounts when no_cache=true
        let rust = build_sidecar_run_preview(
            "tc-rust-nocache",
            Some("aifo-net-x"),
            None,
            "rust",
            "rust:1.80-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let r = shell_join(&rust);
        assert!(
            !r.contains("aifo-cargo-registry:/usr/local/cargo/registry"),
            "unexpected cargo registry mount: {}",
            r
        );
        assert!(
            !r.contains("aifo-cargo-git:/usr/local/cargo/git"),
            "unexpected cargo git mount: {}",
            r
        );

        // node: no npm cache mount
        let node = build_sidecar_run_preview(
            "tc-node-nocache",
            Some("aifo-net-x"),
            None,
            "node",
            "node:20-bookworm-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let n = shell_join(&node);
        assert!(
            !n.contains("aifo-npm-cache:/home/coder/.npm"),
            "unexpected npm cache mount: {}",
            n
        );

        // python: no pip cache mount
        let py = build_sidecar_run_preview(
            "tc-python-nocache",
            Some("aifo-net-x"),
            None,
            "python",
            "python:3.12-slim",
            true,
            pwd,
            Some("docker-default"),
        );
        let p = shell_join(&py);
        assert!(
            !p.contains("aifo-pip-cache:/home/coder/.cache/pip"),
            "unexpected pip cache mount: {}",
            p
        );

        // c-cpp: no ccache volume
        let cpp = build_sidecar_run_preview(
            "tc-cpp-nocache",
            Some("aifo-net-x"),
            None,
            "c-cpp",
            "aifo-cpp-toolchain:latest",
            true,
            pwd,
            Some("docker-default"),
        );
        let c = shell_join(&cpp);
        assert!(
            !c.contains("aifo-ccache:/home/coder/.cache/ccache"),
            "unexpected ccache volume: {}",
            c
        );

        // go: no /go volume
        let go = build_sidecar_run_preview(
            "tc-go-nocache",
            Some("aifo-net-x"),
            None,
            "go",
            "golang:1.22-bookworm",
            true,
            pwd,
            Some("docker-default"),
        );
        let g = shell_join(&go);
        assert!(!g.contains("aifo-go:/go"), "unexpected go volume: {}", g);
    }

    #[test]
    fn test_should_acquire_lock_env() {
        // Default: acquire
        std::env::remove_var("AIFO_CODER_SKIP_LOCK");
        assert!(should_acquire_lock(), "should acquire lock by default");
        // Skip when set to "1"
        std::env::set_var("AIFO_CODER_SKIP_LOCK", "1");
        assert!(
            !should_acquire_lock(),
            "should not acquire lock when AIFO_CODER_SKIP_LOCK=1"
        );
        std::env::remove_var("AIFO_CODER_SKIP_LOCK");
    }

    #[cfg(not(windows))]
    #[test]
    fn test_hashed_lock_path_diff_for_two_repos() {
        // Create two separate repos and ensure their hashed XDG lock paths differ
        let td = tempfile::tempdir().expect("tmpdir");
        let ws = td.path().to_path_buf();
        let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();
        std::env::set_var("XDG_RUNTIME_DIR", &ws);

        // repo A
        let repo_a = ws.join("repo-a");
        std::fs::create_dir_all(&repo_a).unwrap();
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(&repo_a)
            .status();
        std::env::set_current_dir(&repo_a).unwrap();
        let paths_a = candidate_lock_paths();
        assert!(
            paths_a.len() >= 2,
            "expected at least two candidates for repo A"
        );
        let hashed_a = paths_a[1].clone();

        // repo B
        let repo_b = ws.join("repo-b");
        std::fs::create_dir_all(&repo_b).unwrap();
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(&repo_b)
            .status();
        std::env::set_current_dir(&repo_b).unwrap();
        let paths_b = candidate_lock_paths();
        assert!(
            paths_b.len() >= 2,
            "expected at least two candidates for repo B"
        );
        let hashed_b = paths_b[1].clone();

        assert_ne!(
            hashed_a,
            hashed_b,
            "hashed runtime lock path should differ across repos: A={} B={}",
            hashed_a.display(),
            hashed_b.display()
        );

        // restore env/cwd
        if let Some(v) = old_xdg {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
    }

    #[test]
    fn test_candidate_lock_paths_repo_scoped() {
        // Create a temporary git repository and ensure repo-scoped lock paths are preferred
        let td = tempfile::tempdir().expect("tmpdir");
        let old_cwd = std::env::current_dir().expect("cwd");
        let old_xdg = std::env::var("XDG_RUNTIME_DIR").ok();

        // Use a temp runtime dir to make the hashed path predictable and writable
        std::env::set_var("XDG_RUNTIME_DIR", td.path());
        std::env::set_current_dir(td.path()).expect("chdir");

        // Initialize a git repo
        let _ = std::fs::create_dir_all(td.path().join(".git"));
        // Prefer actual git init if available (more realistic)
        let _ = std::process::Command::new("git")
            .arg("init")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        // Resolve repo root (should be Some for initialized repo)
        let root = repo_root().unwrap_or_else(|| td.path().to_path_buf());

        // Compute expected candidates
        let first = root.join(".aifo-coder.lock");
        let key = normalized_repo_key_for_hash(&root);
        let mut second_base = std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        second_base.push(format!(
            "aifo-coder.{}.lock",
            crate::hash_repo_key_hex(&key)
        ));

        let paths = candidate_lock_paths();
        assert_eq!(
            paths.first(),
            Some(&first),
            "first candidate must be in-repo lock path"
        );
        assert_eq!(
            paths.get(1),
            Some(&second_base),
            "second candidate must be hashed runtime-scoped lock path"
        );

        // Restore env and cwd
        if let Some(v) = old_xdg {
            std::env::set_var("XDG_RUNTIME_DIR", v);
        } else {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        std::env::set_current_dir(old_cwd).ok();
    }

    #[cfg(windows)]
    #[test]
    fn test_normalized_repo_key_windows_drive_uppercase_and_backslashes() {
        // Create a temp dir and verify normalization rules:
        // - case-fold whole path
        // - separators are backslashes
        // - drive letter uppercased
        let td = tempfile::tempdir().expect("tmpdir");
        let canon = std::fs::canonicalize(td.path())
            .expect("canon")
            .to_string_lossy()
            .to_string();

        let norm = normalized_repo_key_for_hash(td.path());
        // Build expected normalization from canonical path
        let lower = canon.replace('/', "\\").to_ascii_lowercase();
        let mut expected = lower.into_bytes();
        if expected.len() >= 2 && expected[1] == b':' {
            expected[0] = (expected[0] as char).to_ascii_uppercase() as u8;
        }
        let expected = String::from_utf8(expected).unwrap();
        assert_eq!(norm, expected, "normalized repo key mismatch on Windows");
    }

    #[test]
    fn test_build_docker_cmd_uses_per_pane_state_mounts() {
        // Skip if docker isn't available on this host
        if crate::container_runtime_path().is_err() {
            eprintln!("skipping: docker not found in PATH");
            return;
        }

        let td = tempfile::tempdir().expect("tmpdir");
        let state_dir = td.path().to_path_buf();

        // Save and set env
        let old = std::env::var("AIFO_CODER_FORK_STATE_DIR").ok();
        std::env::set_var("AIFO_CODER_FORK_STATE_DIR", &state_dir);

        let args = vec!["--help".to_string()];
        let (_cmd, preview) =
            crate::build_docker_cmd("aider", &args, "alpine:3.20", None).expect("build_docker_cmd");

        let sd_aider = format!("{}:/home/coder/.aider", state_dir.join(".aider").display());
        let sd_codex = format!("{}:/home/coder/.codex", state_dir.join(".codex").display());
        let sd_crush = format!("{}:/home/coder/.crush", state_dir.join(".crush").display());

        assert!(
            preview.contains(&sd_aider),
            "preview missing per-pane .aider mount: {}",
            preview
        );
        assert!(
            preview.contains(&sd_codex),
            "preview missing per-pane .codex mount: {}",
            preview
        );
        assert!(
            preview.contains(&sd_crush),
            "preview missing per-pane .crush mount: {}",
            preview
        );

        // Ensure home-based mounts for these dirs are not present when per-pane state is set
        if let Some(home) = home::home_dir() {
            let home_aider = format!("{}:/home/coder/.aider", home.join(".aider").display());
            let home_codex = format!("{}:/home/coder/.codex", home.join(".codex").display());
            let home_crush1 = format!(
                "{}:/home/coder/.local/share/crush",
                home.join(".local").join("share").join("crush").display()
            );
            let home_crush2 = format!("{}:/home/coder/.crush", home.join(".crush").display());
            assert!(
                !preview.contains(&home_aider),
                "preview should not include HOME .aider when per-pane state is set: {}",
                preview
            );
            assert!(
                !preview.contains(&home_codex),
                "preview should not include HOME .codex when per-pane state is set: {}",
                preview
            );
            assert!(
                !preview.contains(&home_crush1),
                "preview should not include HOME .local/share/crush when per-pane state is set: {}",
                preview
            );
            assert!(
                !preview.contains(&home_crush2),
                "preview should not include HOME .crush when per-pane state is set: {}",
                preview
            );
        }

        // Restore env
        if let Some(v) = old {
            std::env::set_var("AIFO_CODER_FORK_STATE_DIR", v);
        } else {
            std::env::remove_var("AIFO_CODER_FORK_STATE_DIR");
        }
    }

    // -------------------------
    // Phase 2 unit tests
    // -------------------------

    #[test]
    fn test_fork_sanitize_base_label_rules() {
        assert_eq!(fork_sanitize_base_label("Main Feature"), "main-feature");
        assert_eq!(
            fork_sanitize_base_label("Release/2025.09"),
            "release-2025-09"
        );
        assert_eq!(fork_sanitize_base_label("...Weird__Name///"), "weird-name");
        // Length trimming and trailing cleanup
        let long = "A".repeat(200);
        let s = fork_sanitize_base_label(&long);
        assert!(
            !s.is_empty() && s.len() <= 48,
            "sanitized too long: {}",
            s.len()
        );
    }

    fn have_git() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    // Minimal helper to initialize a git repository with one commit
    fn init_repo(dir: &std::path::Path) {
        let _ = std::process::Command::new("git")
            .arg("init")
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(dir)
            .status();
        // Disable GPG signing to avoid interactive pinentry during test commits
        let _ = std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "tag.gpgSign", "false"])
            .current_dir(dir)
            .status();
        let _ = std::fs::write(dir.join("init.txt"), "x\n");
        let _ = std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir)
            .status();
    }

    #[test]
    fn test_fork_base_info_branch_and_detached() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .expect("git init")
            .success());

        // configure identity (commit-tree does not need it, but normal commit may)
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();

        // make initial commit
        std::fs::write(repo.join("README.md"), "hello\n").expect("write");
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // verify base info on branch
        let (label, base, head) = fork_base_info(repo).expect("base info");
        assert!(!head.is_empty(), "HEAD sha must be non-empty");
        // Default branch could be 'master' or 'main' depending on git config; accept either
        assert!(
            base == "master" || base == "main",
            "expected base to be current branch name, got {}",
            base
        );
        assert!(
            label == "master" || label == "main",
            "expected label to match sanitized branch name, got {}",
            label
        );

        // detached
        assert!(std::process::Command::new("git")
            .args(["checkout", "--detach", "HEAD"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let (label2, base2, head2) = fork_base_info(repo).expect("base info detached");
        assert_eq!(label2, "detached");
        assert_eq!(base2, head2);
    }

    #[test]
    fn test_fork_create_snapshot_commit_exists() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "c1"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // dirty change (unstaged or staged)
        std::fs::write(repo.join("b.txt"), "b\n").unwrap();

        // create snapshot
        let sid = "ut";
        let snap = fork_create_snapshot(repo, sid).expect("snapshot");
        assert_eq!(snap.len(), 40, "snapshot should be a 40-hex sha: {}", snap);

        // verify it's a commit object
        let out = std::process::Command::new("git")
            .arg("cat-file")
            .arg("-t")
            .arg(&snap)
            .current_dir(repo)
            .output()
            .expect("git cat-file");
        let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(
            t, "commit",
            "snapshot object type must be commit, got {}",
            t
        );
    }

    #[test]
    fn test_fork_clone_and_checkout_panes_creates_branches() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("file.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch name
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        let sid = "forksid";
        let res = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(res.len(), 2, "expected two panes");

        // Verify branches are checked out in panes
        for (idx, (pane_dir, branch)) in res.iter().enumerate() {
            assert!(
                pane_dir.exists(),
                "pane dir must exist: {}",
                pane_dir.display()
            );
            let out = std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(pane_dir)
                .output()
                .unwrap();
            let head_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
            assert_eq!(
                &head_branch,
                branch,
                "pane {} HEAD should be {}",
                idx + 1,
                branch
            );
        }
    }

    #[test]
    fn test_fork_merge_fetch_creates_branches_and_meta() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch name and label
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Create a fork session with two panes
        let sid = "sid-merge-fetch";
        let clones = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(clones.len(), 2, "expected two panes");

        // Make independent commits in each pane
        for (idx, (pane_dir, _br)) in clones.iter().enumerate() {
            let fname = format!("pane-{}.txt", idx + 1);
            std::fs::write(pane_dir.join(&fname), format!("pane {}\n", idx + 1)).unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", &format!("pane {}", idx + 1)])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }

        // Perform fetch merge strategy
        let res = fork_merge_branches_by_session(repo, sid, MergingStrategy::Fetch, true, false);
        assert!(
            res.is_ok(),
            "fetch merge strategy should succeed: {:?}",
            res.err()
        );

        // Verify branches exist in the original repo
        for (_pane_dir, branch) in &clones {
            let ok = std::process::Command::new("git")
                .args(["rev-parse", "--verify", branch])
                .current_dir(repo)
                .status()
                .unwrap()
                .success();
            assert!(ok, "expected branch '{}' to exist in original repo", branch);
        }

        // Verify metadata contains merge_strategy=fetch
        let meta_path = repo
            .join(".aifo-coder")
            .join("forks")
            .join(sid)
            .join(".meta.json");
        let meta = std::fs::read_to_string(&meta_path).expect("read meta");
        assert!(
            meta.contains("\"merge_strategy\"") && meta.contains("fetch"),
            "meta should include merge_strategy=fetch, got: {}",
            meta
        );
    }

    #[test]
    fn test_fork_merge_octopus_success_creates_merge_branch_and_deletes_pane_branches() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch name and base label
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Create a fork session with two panes and make non-conflicting commits
        let sid = "sid-merge-oct-success";
        let clones = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(clones.len(), 2, "expected two panes");
        for (idx, (pane_dir, _)) in clones.iter().enumerate() {
            let fname = format!("pane-success-{}.txt", idx + 1);
            std::fs::write(pane_dir.join(&fname), format!("ok {}\n", idx + 1)).unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", &format!("pane ok {}", idx + 1)])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }

        // Perform octopus merge
        let res = fork_merge_branches_by_session(repo, sid, MergingStrategy::Octopus, true, false);
        assert!(res.is_ok(), "octopus merge should succeed: {:?}", res.err());

        // Verify we are on merge/<sid>
        let out2 = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let head_branch = String::from_utf8_lossy(&out2.stdout).trim().to_string();
        assert_eq!(
            head_branch,
            format!("merge/{}", sid),
            "expected HEAD to be merge/<sid>"
        );

        // Verify pane branches are deleted from original repo
        for (_pane_dir, branch) in &clones {
            let ok = std::process::Command::new("git")
                .args(["show-ref", "--verify", &format!("refs/heads/{}", branch)])
                .current_dir(repo)
                .status()
                .unwrap()
                .success();
            assert!(
                !ok,
                "pane branch '{}' should be deleted after octopus merge",
                branch
            );
        }

        // Verify metadata contains merge_target and merge_commit_sha
        let meta_path = repo
            .join(".aifo-coder")
            .join("forks")
            .join(sid)
            .join(".meta.json");
        let meta2 = std::fs::read_to_string(&meta_path).expect("read meta2");
        assert!(
            meta2.contains("\"merge_target\"") && meta2.contains(&format!("merge/{}", sid)),
            "meta should include merge_target=merge/<sid>: {}",
            meta2
        );
        assert!(
            meta2.contains("\"merge_commit_sha\""),
            "meta should include merge_commit_sha: {}",
            meta2
        );
    }

    #[test]
    fn test_fork_merge_octopus_conflict_sets_meta_and_leaves_branches() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with one commit on default branch
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        // Determine current branch
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Create two panes with conflicting changes to the same file
        let sid = "sid-merge-oct-conflict";
        let clones = fork_clone_and_checkout_panes(repo, sid, 2, &cur_branch, &base_label, false)
            .expect("clone panes");
        assert_eq!(clones.len(), 2, "expected two panes");

        // Pane 1 writes conflict.txt
        {
            let (pane_dir, _) = &clones[0];
            std::fs::write(pane_dir.join("conflict.txt"), "A\n").unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", "pane1"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }
        // Pane 2 writes conflicting content
        {
            let (pane_dir, _) = &clones[1];
            std::fs::write(pane_dir.join("conflict.txt"), "B\n").unwrap();
            let _ = std::process::Command::new("git")
                .args(["config", "user.name", "AIFO Test"])
                .current_dir(pane_dir)
                .status();
            let _ = std::process::Command::new("git")
                .args(["config", "user.email", "aifo@example.com"])
                .current_dir(pane_dir)
                .status();
            assert!(std::process::Command::new("git")
                .args(["add", "-A"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
            assert!(std::process::Command::new("git")
                .args(["commit", "-m", "pane2"])
                .current_dir(pane_dir)
                .status()
                .unwrap()
                .success());
        }

        // Attempt octopus merge (should fail due to conflict)
        let res = fork_merge_branches_by_session(repo, sid, MergingStrategy::Octopus, true, false);
        assert!(res.is_err(), "octopus merge should fail due to conflicts");

        // Metadata should record merge_failed: true
        let meta_path = repo
            .join(".aifo-coder")
            .join("forks")
            .join(sid)
            .join(".meta.json");
        let meta = std::fs::read_to_string(&meta_path).expect("read meta");
        assert!(
            meta.contains("\"merge_failed\":true"),
            "meta should include merge_failed:true, got: {}",
            meta
        );

        // Fetched pane branches should exist in original repo (not deleted)
        for (_pane_dir, branch) in &clones {
            let ok = std::process::Command::new("git")
                .args(["show-ref", "--verify", &format!("refs/heads/{}", branch)])
                .current_dir(repo)
                .status()
                .unwrap()
                .success();
            assert!(
                ok,
                "pane branch '{}' should exist after failed merge",
                branch
            );
        }

        // Repo should be in conflict state (has unmerged paths)
        let out2 = std::process::Command::new("git")
            .args(["ls-files", "-u"])
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            !out2.stdout.is_empty(),
            "expected unmerged paths after failed octopus merge"
        );
    }

    #[test]
    fn test_fork_clone_and_checkout_panes_inits_submodules() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");

        // Create submodule repository
        let sub = td.path().join("sm");
        std::fs::create_dir_all(&sub).expect("mkdir sm");
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&sub)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&sub)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&sub)
            .status();
        std::fs::write(sub.join("sub.txt"), "sub\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&sub)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "sub init"])
            .current_dir(&sub)
            .status()
            .unwrap()
            .success());

        // Create base repository and add submodule
        let base = td.path().join("base");
        std::fs::create_dir_all(&base).expect("mkdir base");
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(&base)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(&base)
            .status();
        std::fs::write(base.join("file.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "base init"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());

        // Add submodule pointing to local path; allow file transport explicitly for modern Git
        let sub_path = sub.display().to_string();
        assert!(std::process::Command::new("git")
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &sub_path,
                "submod"
            ])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "add submodule"])
            .current_dir(&base)
            .status()
            .unwrap()
            .success());

        // Determine current branch name
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&base)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Clone panes and ensure submodule is initialized in clone
        let res =
            fork_clone_and_checkout_panes(&base, "sid-sub", 1, &cur_branch, &base_label, false)
                .expect("clone panes with submodule");
        assert_eq!(res.len(), 1);
        let pane_dir = &res[0].0;
        let sub_file = pane_dir.join("submod").join("sub.txt");
        assert!(
            sub_file.exists(),
            "expected submodule file to exist in clone: {}",
            sub_file.display()
        );
    }

    #[test]
    fn test_fork_clone_and_checkout_panes_lfs_marker_does_not_fail_without_lfs() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();

        // init repo with a .gitattributes marking LFS filters (may or may not have git-lfs installed)
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();

        std::fs::write(
            repo.join(".gitattributes"),
            "*.bin filter=lfs diff=lfs merge=lfs -text\n",
        )
        .unwrap();
        std::fs::write(repo.join("a.bin"), b"\x00\x01\x02").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        // Commit even if lfs not installed; the filter may be ignored, but commit should succeed
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "add lfs marker"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());

        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);

        // Should not fail regardless of git-lfs availability
        let res =
            fork_clone_and_checkout_panes(repo, "sid-lfs", 1, &cur_branch, &base_label, false)
                .expect("clone panes with lfs marker");
        assert_eq!(res.len(), 1);
    }

    #[test]
    fn test_repo_uses_lfs_quick_top_level_gitattributes() {
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        // Create top-level .gitattributes with lfs filter
        std::fs::write(
            repo.join(".gitattributes"),
            "*.bin filter=lfs diff=lfs merge=lfs -text\n",
        )
        .unwrap();
        assert!(
            repo_uses_lfs_quick(repo),
            "expected repo_uses_lfs_quick to detect top-level filter=lfs"
        );
    }

    #[test]
    fn test_repo_uses_lfs_quick_nested_gitattributes() {
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        let nested = repo.join("assets").join("media");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join(".gitattributes"),
            "*.png filter=lfs diff=lfs merge=lfs -text\n",
        )
        .unwrap();
        assert!(
            repo_uses_lfs_quick(repo),
            "expected repo_uses_lfs_quick to detect nested filter=lfs"
        );
    }

    #[test]
    fn test_repo_uses_lfs_quick_lfsconfig_present() {
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        std::fs::write(
            repo.join(".lfsconfig"),
            "[lfs]\nurl = https://example.com/lfs\n",
        )
        .unwrap();
        assert!(
            repo_uses_lfs_quick(repo),
            "expected repo_uses_lfs_quick to detect .lfsconfig presence"
        );
    }

    #[test]
    fn test_fork_clean_protects_ahead_and_force_deletes() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();

        // Session/pane setup
        let sid = "sid-ahead";
        let base = root.join(".aifo-coder").join("forks").join(sid);
        let pane = base.join("pane-1");
        std::fs::create_dir_all(&pane).unwrap();
        init_repo(&pane);

        // Record base_commit_sha as current HEAD
        let head = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap();
        let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();

        // Write minimal meta.json
        std::fs::create_dir_all(&base).unwrap();
        let meta = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            head_sha,
            pane.display()
        );
        std::fs::write(base.join(".meta.json"), meta).unwrap();

        // Create an extra commit in the pane to make it "ahead" of base_commit_sha
        std::fs::write(pane.join("new.txt"), "y\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "advance pane"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        // Default clean should REFUSE because pane is ahead
        let opts_refuse = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: false,
            force: false,
            keep_dirty: false,
            json: false,
        };
        let code = aifo_coder::fork_clean(&root, &opts_refuse).expect("fork_clean refuse");
        assert_eq!(code, 1, "expected refusal when pane is ahead");
        assert!(base.exists(), "session dir must remain after refusal");

        // keep-dirty should succeed, keep the ahead pane and update meta
        let opts_keep = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: true, // skip prompt
            force: false,
            keep_dirty: true,
            json: false,
        };
        let code2 = aifo_coder::fork_clean(&root, &opts_keep).expect("fork_clean keep-dirty");
        assert_eq!(
            code2, 0,
            "keep-dirty should succeed (no deletions if all panes protected)"
        );
        assert!(pane.exists(), "ahead pane should remain");
        let meta2 = std::fs::read_to_string(base.join(".meta.json")).expect("read meta2");
        assert!(
            meta2.contains("\"panes_remaining\""),
            "meta should be updated to include panes_remaining"
        );

        // force should delete the session
        let opts_force = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: true,
            force: true,
            keep_dirty: false,
            json: false,
        };
        let code3 = aifo_coder::fork_clean(&root, &opts_force).expect("fork_clean force");
        assert_eq!(code3, 0, "force should succeed");
        assert!(!base.exists(), "session dir should be removed by force");
    }

    #[test]
    fn test_fork_clean_protects_submodule_dirty() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();

        // Prepare submodule upstream repo
        let upstream = td.path().join("upstream-sm");
        std::fs::create_dir_all(&upstream).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&upstream)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(&upstream)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(&upstream)
            .status();
        std::fs::write(upstream.join("a.txt"), "a\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&upstream)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "sm init"])
            .current_dir(&upstream)
            .status()
            .unwrap()
            .success());

        // Create pane repo and add submodule
        let sid = "sid-subdirty";
        let base = root.join(".aifo-coder").join("forks").join(sid);
        let pane = base.join("pane-1");
        std::fs::create_dir_all(&pane).unwrap();
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(&pane)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(&pane)
            .status();
        // Commit initial file
        std::fs::write(pane.join("root.txt"), "r\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "root"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        // Add submodule (allow file protocol)
        let up_path = upstream.display().to_string();
        assert!(std::process::Command::new("git")
            .args([
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                &up_path,
                "sub"
            ])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "add submodule"])
            .current_dir(&pane)
            .status()
            .unwrap()
            .success());

        // Record base_commit_sha as current HEAD in pane
        let head = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "HEAD"])
            .current_dir(&pane)
            .output()
            .unwrap();
        let head_sha = String::from_utf8_lossy(&head.stdout).trim().to_string();
        std::fs::create_dir_all(&base).unwrap();
        let meta = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            head_sha,
            pane.display()
        );
        std::fs::write(base.join(".meta.json"), meta).unwrap();

        // Make submodule dirty relative to recorded commit: commit new change inside pane/sub (the submodule checkout)
        let subdir = pane.join("sub");
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "UT"])
            .current_dir(&subdir)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "ut@example.com"])
            .current_dir(&subdir)
            .status();
        std::fs::write(subdir.join("b.txt"), "b\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&subdir)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "advance sub"])
            .current_dir(&subdir)
            .status()
            .unwrap()
            .success());

        // Default clean should refuse due to submodules-dirty
        let opts_refuse = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: false,
            force: false,
            keep_dirty: false,
            json: false,
        };
        let code =
            aifo_coder::fork_clean(&root, &opts_refuse).expect("fork_clean refuse submodule-dirty");
        assert_eq!(code, 1, "expected refusal when submodule is dirty");
        assert!(
            base.exists(),
            "session dir must remain after refusal on submodule-dirty"
        );

        // keep-dirty should keep the pane and succeed
        let opts_keep = aifo_coder::ForkCleanOpts {
            session: Some(sid.to_string()),
            older_than_days: None,
            all: false,
            dry_run: false,
            yes: true,
            force: false,
            keep_dirty: true,
            json: false,
        };
        let code2 =
            aifo_coder::fork_clean(&root, &opts_keep).expect("fork_clean keep-dirty submodule");
        assert_eq!(
            code2, 0,
            "keep-dirty should succeed (no deletions if pane protected)"
        );
        assert!(pane.exists(), "pane with dirty submodule should remain");
    }

    #[test]
    fn test_fork_clean_older_than_deletes_only_old_sessions() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        init_repo(&root);

        // Old clean session (older than threshold)
        let sid_old = "sid-old2";
        let base_old = root.join(".aifo-coder").join("forks").join(sid_old);
        let pane_old = base_old.join("pane-1");
        std::fs::create_dir_all(&pane_old).unwrap();
        init_repo(&pane_old);
        let head_old = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_old)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let old_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 20 * 86400;
        let meta_old = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            old_secs, head_old, pane_old.display(), sid = sid_old
        );
        std::fs::create_dir_all(&base_old).unwrap();
        std::fs::write(base_old.join(".meta.json"), meta_old).unwrap();

        // Recent clean session (younger than threshold)
        let sid_new = "sid-new2";
        let base_new = root.join(".aifo-coder").join("forks").join(sid_new);
        let pane_new = base_new.join("pane-1");
        std::fs::create_dir_all(&pane_new).unwrap();
        init_repo(&pane_new);
        let head_new = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_new)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let meta_new = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            now_secs, head_new, pane_new.display(), sid = sid_new
        );
        std::fs::create_dir_all(&base_new).unwrap();
        std::fs::write(base_new.join(".meta.json"), meta_new).unwrap();

        // Clean with older-than=10 days should delete only sid-old2
        let opts = aifo_coder::ForkCleanOpts {
            session: None,
            older_than_days: Some(10),
            all: false,
            dry_run: false,
            yes: true,
            force: false,
            keep_dirty: false,
            json: false,
        };
        let code = aifo_coder::fork_clean(&root, &opts).expect("fork_clean older-than");
        assert_eq!(code, 0, "older-than clean should succeed");
        assert!(!base_old.exists(), "old session should be deleted");
        assert!(base_new.exists(), "recent session should remain");
    }

    #[test]
    fn test_fork_create_snapshot_on_empty_repo() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        // Create an untracked file; snapshot should still succeed by indexing working tree
        std::fs::write(repo.join("a.txt"), "a\n").unwrap();
        let sid = "empty";
        let snap = fork_create_snapshot(repo, sid).expect("snapshot on empty repo");
        assert_eq!(snap.len(), 40, "snapshot sha length");
        let out = std::process::Command::new("git")
            .args(["cat-file", "-t", &snap])
            .current_dir(repo)
            .output()
            .unwrap();
        let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(t, "commit", "snapshot object must be a commit");
    }

    #[test]
    fn test_fork_clone_with_dissociate() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let repo = td.path();
        // init repo with one commit
        assert!(std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        let _ = std::process::Command::new("git")
            .args(["config", "user.name", "AIFO Test"])
            .current_dir(repo)
            .status();
        let _ = std::process::Command::new("git")
            .args(["config", "user.email", "aifo@example.com"])
            .current_dir(repo)
            .status();
        std::fs::write(repo.join("f.txt"), "x\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        // Determine branch
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(repo)
            .output()
            .unwrap();
        let cur_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let base_label = fork_sanitize_base_label(&cur_branch);
        // Clone with dissociate should succeed
        let res =
            fork_clone_and_checkout_panes(repo, "sid-dissoc", 1, &cur_branch, &base_label, true)
                .expect("clone with --dissociate");
        assert_eq!(res.len(), 1);
        assert!(res[0].0.exists());
    }

    #[test]
    fn test_fork_autoclean_removes_only_clean_sessions() {
        if !have_git() {
            eprintln!("skipping: git not found in PATH");
            return;
        }
        let td = tempfile::tempdir().expect("tmpdir");
        let root = td.path().to_path_buf();
        // Initialize base repo to ensure repo_root() detects it
        init_repo(&root);

        // Old clean session
        let sid_clean = "sid-clean-old";
        let base_clean = root.join(".aifo-coder").join("forks").join(sid_clean);
        let pane_clean = base_clean.join("pane-1");
        std::fs::create_dir_all(&pane_clean).unwrap();
        init_repo(&pane_clean);
        // Record base_commit_sha as current HEAD
        let head_clean = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_clean)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        let old_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - 40 * 86400;
        std::fs::create_dir_all(&base_clean).unwrap();
        let meta_clean = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            old_secs, head_clean, pane_clean.display(), sid = sid_clean
        );
        std::fs::write(base_clean.join(".meta.json"), meta_clean).unwrap();

        // Old protected (ahead) session
        let sid_prot = "sid-protected-old";
        let base_prot = root.join(".aifo-coder").join("forks").join(sid_prot);
        let pane_prot = base_prot.join("pane-1");
        std::fs::create_dir_all(&pane_prot).unwrap();
        init_repo(&pane_prot);
        let head_prot = String::from_utf8_lossy(
            &std::process::Command::new("git")
                .args(["rev-parse", "--verify", "HEAD"])
                .current_dir(&pane_prot)
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_string();
        std::fs::create_dir_all(&base_prot).unwrap();
        let meta_prot = format!(
            "{{ \"created_at\": {}, \"base_label\": \"main\", \"base_ref_or_sha\": \"main\", \"base_commit_sha\": \"{}\", \"panes\": 1, \"pane_dirs\": [\"{}\"], \"branches\": [\"fork/main/{sid}-1\"], \"layout\": \"tiled\" }}",
            old_secs, head_prot, pane_prot.display(), sid = sid_prot
        );
        std::fs::write(base_prot.join(".meta.json"), meta_prot).unwrap();
        // Make pane ahead of base_commit_sha
        std::fs::write(pane_prot.join("new.txt"), "y\n").unwrap();
        assert!(std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&pane_prot)
            .status()
            .unwrap()
            .success());
        assert!(std::process::Command::new("git")
            .args(["commit", "-m", "advance pane"])
            .current_dir(&pane_prot)
            .status()
            .unwrap()
            .success());

        // Run autoclean with threshold 1 day
        let old_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&root).unwrap();
        let old_env1 = std::env::var("AIFO_CODER_FORK_AUTOCLEAN").ok();
        let old_env2 = std::env::var("AIFO_CODER_FORK_STALE_DAYS").ok();
        std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", "1");
        std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", "1");
        fork_autoclean_if_enabled();
        // Restore cwd and env
        std::env::set_current_dir(old_cwd).ok();
        if let Some(v) = old_env1 {
            std::env::set_var("AIFO_CODER_FORK_AUTOCLEAN", v);
        } else {
            std::env::remove_var("AIFO_CODER_FORK_AUTOCLEAN");
        }
        if let Some(v) = old_env2 {
            std::env::set_var("AIFO_CODER_FORK_STALE_DAYS", v);
        } else {
            std::env::remove_var("AIFO_CODER_FORK_STALE_DAYS");
        }

        assert!(
            !base_clean.exists(),
            "clean old session should have been deleted by autoclean"
        );
        assert!(
            base_prot.exists(),
            "protected old session should have been kept by autoclean"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_windows_helpers_orient_and_builders() {
        use crate::{
            fork_bash_inner_string, fork_ps_inner_string, wt_build_new_tab_args,
            wt_build_split_args, wt_orient_for_layout,
        };
        let agent = "aider";
        let sid = "sidw";
        let tmp = tempfile::tempdir().expect("tmpdir");
        let pane_dir = tmp.path().join("p");
        std::fs::create_dir_all(&pane_dir).unwrap();
        let state_dir = tmp.path().join("s");
        std::fs::create_dir_all(&state_dir).unwrap();
        let child = vec!["aider".to_string(), "--help".to_string()];
        let ps = fork_ps_inner_string(agent, sid, 1, &pane_dir, &state_dir, &child);
        assert!(
            ps.contains("Set-Location '"),
            "ps inner should set location: {}",
            ps
        );
        assert!(
            ps.contains("$env:AIFO_CODER_SKIP_LOCK='1'"),
            "ps inner should set env"
        );
        let bash = fork_bash_inner_string(agent, sid, 2, &pane_dir, &state_dir, &child);
        assert!(bash.contains("cd "), "bash inner should cd");
        assert!(
            bash.contains("export AIFO_CODER_SKIP_LOCK='1'"),
            "bash inner export env"
        );
        // wt orientation
        assert_eq!(wt_orient_for_layout("even-h", 3), "-H");
        assert_eq!(wt_orient_for_layout("even-v", 4), "-V");
        // tiled alternates
        let o2 = wt_orient_for_layout("tiled", 2);
        let o3 = wt_orient_for_layout("tiled", 3);
        assert!(o2 == "-H" || o2 == "-V");
        assert!(o3 == "-H" || o3 == "-V");
        // arg builders
        let psbin = std::path::PathBuf::from("powershell.exe");
        let inner = "cmds";
        let newtab = wt_build_new_tab_args(&psbin, &pane_dir, inner);
        assert_eq!(newtab[0], "wt");
        assert_eq!(newtab[1], "new-tab");
        let split = wt_build_split_args("-H", &psbin, &pane_dir, inner);
        assert_eq!(split[1], "split-pane");
        assert_eq!(split[2], "-H");
        // Wait-Process cmd builder
        let w = crate::ps_wait_process_cmd(&["101", "202", "303"]);
        assert_eq!(w, "Wait-Process -Id 101,202,303");
    }
}
