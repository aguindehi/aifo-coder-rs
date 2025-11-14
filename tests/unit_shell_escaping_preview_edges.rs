use aifo_coder as aifo;

/// Extra shell escaping edge cases beyond helpers.rs
#[test]
fn unit_shell_escape_edges() {
    assert_eq!(aifo::shell_escape(""), "''");
    assert_eq!(aifo::shell_escape("abc-123_./:@"), "abc-123_./:@");
    assert_eq!(aifo::shell_escape("a b"), "'a b'");
    assert_eq!(aifo::shell_escape("O'Reilly"), "'O'\"'\"'Reilly'");
}

