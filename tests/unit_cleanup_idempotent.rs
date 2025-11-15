#[test]
fn unit_test_cleanup_with_random_session_id_is_noop() {
    // Should not panic or print daemon errors
    let sid = format!("no-such-session-{}", std::process::id());
    aifo_coder::toolchain_cleanup_session(&sid, true);
    aifo_coder::toolchain_cleanup_session(&sid, false);
    // If we reach here without panic, it's fine.
}
