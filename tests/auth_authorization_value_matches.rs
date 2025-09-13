use aifo_coder::authorization_value_matches;

#[test]
fn test_bearer_scheme_case_insensitive_and_exact_token() {
    let tok = "abc123";
    assert!(authorization_value_matches("Bearer abc123", tok));
    assert!(authorization_value_matches("bearer abc123", tok));
    assert!(authorization_value_matches("BEARER abc123", tok));
}

#[test]
fn test_whitespace_tolerance_common_patterns() {
    let tok = "t0k3n";
    // Leading/trailing spaces around header value
    assert!(authorization_value_matches("  Bearer t0k3n  ", tok));
    // Canonical single space
    assert!(authorization_value_matches("Bearer t0k3n", tok));
    // Multiple spaces should be tolerated by the parser
    assert!(authorization_value_matches("Bearer    t0k3n", tok));
}

#[test]
fn test_wrong_token_and_wrong_scheme() {
    let tok = "needle";
    assert!(!authorization_value_matches("Bearer haystack", tok));
    assert!(!authorization_value_matches("Basic needle", tok));
    assert!(!authorization_value_matches("Token needle", tok));
}

#[test]
fn test_punctuation_is_exact_match() {
    let tok = "tok-en_1.2/3";
    assert!(authorization_value_matches("Bearer tok-en_1.2/3", tok));
    assert!(!authorization_value_matches("Bearer tok-en_1.2/3x", tok));
}
