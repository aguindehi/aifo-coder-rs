#[test]
fn fork_sanitize_base_label_property_like() {
    // Deterministic set of mixed strings
    let cases = vec![
        "Hello, World!",
        "UPPER_lower-123.__/\\__   mixed---",
        "***invalid***chars###",
        "a".repeat(200).as_str(),
        "-leading-and-trailing-",
        " spaced  words ",
        "symbols_./-group",
    ];

    for s in cases {
        let out = aifo_coder::fork_sanitize_base_label(s);
        // Only [a-z0-9-]
        assert!(
            out.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "sanitized contains invalid chars: {} -> {}",
            s,
            out
        );
        // Length <= 48
        assert!(
            out.len() <= 48,
            "sanitized length too long ({}): {} -> {}",
            out.len(),
            s,
            out
        );
        // No leading/trailing '-'
        if !out.is_empty() {
            assert!(out.as_bytes().first() != Some(&b'-'), "leading '-': {}", out);
            assert!(out.as_bytes().last() != Some(&b'-'), "trailing '-': {}", out);
        }
    }
}
