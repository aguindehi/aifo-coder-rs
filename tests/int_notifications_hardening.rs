mod support;

#[cfg(unix)]
mod notifications_hardening_tests {
    use crate::support;
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::symlink;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn write_executable_script(path: &std::path::Path, body: &str) {
        let mut f = fs::File::create(path).expect("create script");
        f.write_all(body.as_bytes()).expect("write script");
        let mut perm = fs::metadata(path).expect("meta").permissions();
        perm.set_mode(0o755);
        fs::set_permissions(path, perm).expect("chmod");
    }

    #[test]
    fn int_notifications_canonicalization_symlink_basename_allowlist() {
        let td = tempdir().expect("tmpdir");
        let dir = td.path();

        // Real script named 'say'
        let script = dir.join("say");
        let script_body = "#!/bin/sh\necho ok\nexit 0\n";
        write_executable_script(&script, script_body);

        // Symlink to the script with a different basename
        let link = dir.join("say-link");
        symlink(&script, &link).expect("symlink");

        // YAML config pointing to the symlink; trailing {args} placeholder
        let cfg_path = dir.join("aider.conf.yml");
        let yaml = format!(
            "notifications-command:\n  - \"{}\"\n  - \"{{args}}\"\n",
            link.display()
        );
        fs::write(&cfg_path, yaml).expect("write yaml");

        // Point notifications config to our temp file
        let _env_guard = support::notifications_allow_test_exec_from(dir);
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);
        // Ensure default (no trimming) to avoid env noise
        std::env::remove_var("AIFO_NOTIFICATIONS_TRIM_ENV");
        std::env::remove_var("AIFO_NOTIFICATIONS_ENV_ALLOW");

        // Request uses cmd='say' (allowlist default) which should match canonicalized basename
        let argv: Vec<String> = Vec::new();
        let res = aifo_coder::notifications_handle_request(&argv, false, 2)
            .expect("handle request via symlink canonicalized");
        let (code, body) = res;
        assert_eq!(code, 0, "expected exit 0, got {}", code);
        let out = String::from_utf8_lossy(&body).to_string();
        assert!(
            out.contains("ok"),
            "expected script output to contain 'ok', got: {}",
            out
        );

        // Cleanup env
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
    }

    #[test]
    fn int_notifications_trim_env_and_allowlist() {
        let td = tempdir().expect("tmpdir");
        let dir = td.path();

        // Real script named 'say' that optionally prints SECRET_VAR
        let script = dir.join("say");
        let script_body = r#"#!/bin/sh
case "$1" in
  print-secret) echo "${SECRET_VAR:-missing}" ;;
  *) echo "ok" ;;
esac
exit 0
"#;
        write_executable_script(&script, script_body);

        // YAML config pointing to the script with trailing {args}
        let cfg_path = dir.join("aider.conf.yml");
        let yaml = format!(
            "notifications-command:\n  - \"{}\"\n  - \"{{args}}\"\n",
            script.display()
        );
        fs::write(&cfg_path, yaml).expect("write yaml");
        let _env_guard = support::notifications_allow_test_exec_from(dir);
        std::env::set_var("AIFO_NOTIFICATIONS_CONFIG", &cfg_path);

        // Set a secret env var in parent
        std::env::set_var("SECRET_VAR", "topsecret");

        // Enable trimming: SECRET_VAR should be dropped unless explicitly allowed
        std::env::set_var("AIFO_NOTIFICATIONS_TRIM_ENV", "1");
        std::env::remove_var("AIFO_NOTIFICATIONS_ENV_ALLOW");

        // Invoke with arg to print the secret; expect 'missing'
        let argv = vec!["print-secret".to_string()];
        let (code1, body1) =
            aifo_coder::notifications_handle_request(&argv, false, 2).expect("trim run");
        assert_eq!(code1, 0, "expected exit 0 on trimmed run");
        let out1 = String::from_utf8_lossy(&body1).trim().to_string();
        assert_eq!(
            out1, "missing",
            "expected SECRET_VAR to be missing when TRIM_ENV=1 without allowlist"
        );

        // Allow SECRET_VAR explicitly and expect it to pass through
        std::env::set_var("AIFO_NOTIFICATIONS_ENV_ALLOW", "SECRET_VAR");
        let (code2, body2) =
            aifo_coder::notifications_handle_request(&argv, false, 2).expect("allowed run");
        assert_eq!(code2, 0, "expected exit 0 on allowed run");
        let out2 = String::from_utf8_lossy(&body2).trim().to_string();
        assert_eq!(
            out2, "topsecret",
            "expected SECRET_VAR to be preserved when allowed explicitly"
        );

        // Cleanup env
        std::env::remove_var("AIFO_NOTIFICATIONS_CONFIG");
        std::env::remove_var("AIFO_NOTIFICATIONS_TRIM_ENV");
        std::env::remove_var("AIFO_NOTIFICATIONS_ENV_ALLOW");
        std::env::remove_var("SECRET_VAR");
    }
}
