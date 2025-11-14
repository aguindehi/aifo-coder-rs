use aifo_coder as aifo;

/// Extra shell escaping edge cases beyond helpers.rs
#[test]
fn test_shell_escape_edges() {
    assert_eq!(aifo::shell_escape(""), "''");
    assert_eq!(aifo::shell_escape("abc-123_./:@"), "abc-123_./:@");
    assert_eq!(aifo::shell_escape("a b"), "'a b'");
    assert_eq!(aifo::shell_escape("O'Reilly"), "'O'\"'\"'Reilly'");
}

/// Ensure docker preview string includes properly escaped agent args.
#[test]
fn test_preview_shell_escaping_args() {
    // Skip if docker isn't available on this host
    if aifo::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }
    let args = vec!["arg with space".to_string(), "O'Reilly".to_string()];
    let image = "alpine:3.20";
    let (_cmd, preview) =
        aifo::build_docker_cmd("crush", &args, image, None).expect("build_docker_cmd failed");
    assert!(
        preview.contains("'arg with space'"),
        "preview missing escaped space: {preview}"
    );
    // Inside the single-quoted sh -lc script, a literal single-quote is represented by: '"'"'
    assert!(
        preview.contains("'\"'\"'"),
        "preview missing escaped single quote sequence ('\"'\"'): {preview}"
    );
}
