use std::fs;
use std::path::PathBuf;
use aifo_coder as crate;

#[test]
fn test_shell_escape_simple() {
    assert_eq!(crate::shell_escape("abc-123_./:@"), "abc-123_./:@");
}

#[test]
fn test_shell_escape_with_spaces_and_quotes() {
    assert_eq!(crate::shell_escape("a b c"), "'a b c'");
    assert_eq!(crate::shell_escape("O'Reilly"), "'O'\"'\"'Reilly'");
}

#[test]
fn test_shell_join() {
    let args = vec!["a".to_string(), "b c".to_string(), "d".to_string()];
    assert_eq!(crate::shell_join(&args), "a 'b c' d");
}

#[test]
fn test_path_pair() {
    let host = PathBuf::from("/tmp");
    let os = crate::path_pair(&host, "/container");
    assert_eq!(os.to_string_lossy(), "/tmp:/container");
}

#[test]
fn test_ensure_file_exists_creates() {
    let mut p = std::env::temp_dir();
    p.push(format!("aifo-coder-test-{}", std::process::id()));
    p.push("nested");
    let _ = fs::remove_dir_all(&p.parent().unwrap()); // best-effort cleanup
    let _ = crate::ensure_file_exists(&p); // should create parent and file
    assert!(p.exists());
    // cleanup
    let _ = fs::remove_file(&p);
    let _ = fs::remove_dir_all(p.parent().unwrap());
}

#[test]
fn test_candidate_lock_paths_contains_tmp() {
    let paths = crate::candidate_lock_paths();
    assert!(paths.iter().any(|pp| pp == &PathBuf::from("/tmp/aifo-coder.lock")));
}

#[test]
fn test_desired_apparmor_profile_option() {
    // This is a smoke test that adapts to environment capabilities.
    let prof = crate::desired_apparmor_profile();
    if crate::docker_supports_apparmor() {
        assert!(prof.is_some());
    } else {
        assert!(prof.is_none());
    }
}
