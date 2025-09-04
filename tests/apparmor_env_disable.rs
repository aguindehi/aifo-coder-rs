use std::env;

#[test]
fn test_apparmor_env_disable_results_in_none() {
    let old = env::var("AIFO_CODER_APPARMOR_PROFILE").ok();
    env::set_var("AIFO_CODER_APPARMOR_PROFILE", "none");
    let prof = aifo_coder::desired_apparmor_profile_quiet();
    // Restore env
    if let Some(v) = old {
        env::set_var("AIFO_CODER_APPARMOR_PROFILE", v);
    } else {
        env::remove_var("AIFO_CODER_APPARMOR_PROFILE");
    }
    assert!(
        prof.is_none(),
        "expected AppArmor profile to be disabled via env=none, got: {:?}",
        prof
    );
}
