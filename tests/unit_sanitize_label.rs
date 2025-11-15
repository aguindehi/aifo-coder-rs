use aifo_coder::fork_sanitize_base_label;

#[test]
fn unit_sanitize_basic_lowercase() {
    assert_eq!(fork_sanitize_base_label("Main"), "main");
    assert_eq!(fork_sanitize_base_label("FEATURE"), "feature");
}

#[test]
fn unit_sanitize_separators_and_collapse() {
    assert_eq!(fork_sanitize_base_label("feature/XYZ"), "feature-xyz");
    assert_eq!(
        fork_sanitize_base_label("Feature: Cool  Stuff!"),
        "feature-cool-stuff"
    );
    assert_eq!(fork_sanitize_base_label("a//b__c..d"), "a-b-c-d");
}

#[test]
fn unit_sanitize_trim_edges() {
    assert_eq!(fork_sanitize_base_label("/-._Hello_-./"), "hello");
    assert_eq!(fork_sanitize_base_label("..foo.."), "foo");
    assert_eq!(fork_sanitize_base_label("--bar--"), "bar");
}

#[test]
fn unit_sanitize_max_length() {
    let s = "a".repeat(100);
    let out = fork_sanitize_base_label(&s);
    assert!(
        out.len() <= 48,
        "sanitized label should be <= 48 chars, got {}",
        out.len()
    );
    assert!(out.chars().all(|c| c == 'a'), "expected only 'a's: {}", out);
}
