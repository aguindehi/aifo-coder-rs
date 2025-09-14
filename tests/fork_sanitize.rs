use aifo_coder as aifo;

#[test]
fn test_fork_sanitize_base_label_rules() {
    assert_eq!(
        aifo::fork_sanitize_base_label("Main Feature"),
        "main-feature"
    );
    assert_eq!(
        aifo::fork_sanitize_base_label("Release/2025.09"),
        "release-2025-09"
    );
    assert_eq!(
        aifo::fork_sanitize_base_label("...Weird__Name///"),
        "weird-name"
    );

    // Length trimming and trailing cleanup
    let long = "A".repeat(200);
    let s = aifo::fork_sanitize_base_label(&long);
    assert!(
        !s.is_empty() && s.len() <= 48,
        "sanitized too long: {}",
        s.len()
    );
}
