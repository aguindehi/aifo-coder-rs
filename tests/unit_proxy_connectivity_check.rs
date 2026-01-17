use std::env;

fn stash_proxy_env() -> Vec<(String, Option<String>)> {
    let keys = ["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"];
    keys.iter()
        .map(|k| (k.to_string(), env::var(k).ok()))
        .collect()
}

fn restore_proxy_env(prev: &[(String, Option<String>)]) {
    for (k, v) in prev {
        match v {
            Some(val) => env::set_var(k, val),
            None => env::remove_var(k),
        }
    }
}

#[test]
fn unit_proxy_connectivity_marks_unreachable_and_clears_env() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev = stash_proxy_env();
    let prev_fallback = env::var("AIFO_PROXY_FALLBACK").ok();
    env::set_var("http_proxy", "http://example.invalid:9");
    env::set_var("https_proxy", "https://example.invalid:9");
    env::set_var("no_proxy", "127.0.0.1,localhost");

    let outcome = aifo_coder::proxy::proxy_connectivity_check_with(|_host, _port| false);

    assert_eq!(
        outcome,
        aifo_coder::proxy::ProxyCheckOutcome::Cleared(vec![
            "http_proxy".to_string(),
            "https_proxy".to_string()
        ])
    );
    assert_eq!(env::var("http_proxy").unwrap_or_default(), "");
    assert_eq!(env::var("https_proxy").unwrap_or_default(), "");
    assert_eq!(env::var("HTTP_PROXY").unwrap_or_default(), "");
    assert_eq!(env::var("HTTPS_PROXY").unwrap_or_default(), "");
    assert_eq!(env::var("no_proxy").unwrap_or_default(), "");
    assert_eq!(env::var("NO_PROXY").unwrap_or_default(), "");
    assert!(aifo_coder::proxy::should_force_direct_proxy());

    match prev_fallback {
        Some(v) => env::set_var("AIFO_PROXY_FALLBACK", v),
        None => env::remove_var("AIFO_PROXY_FALLBACK"),
    }
    restore_proxy_env(&prev);
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_proxy_connectivity_retains_when_probe_succeeds() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev = stash_proxy_env();
    env::set_var("http_proxy", "http://example.invalid:9");

    let outcome = aifo_coder::proxy::proxy_connectivity_check_with(|_host, _port| true);

    assert_eq!(outcome, aifo_coder::proxy::ProxyCheckOutcome::Retained);
    assert_eq!(
        env::var("http_proxy").unwrap_or_default(),
        "http://example.invalid:9"
    );
    assert!(!aifo_coder::proxy::should_force_direct_proxy());

    restore_proxy_env(&prev);
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_proxy_connectivity_skips_when_fallback_disabled() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev = stash_proxy_env();
    let prev_toggle = env::var("AIFO_PROXY_FALLBACK").ok();
    env::set_var("http_proxy", "http://example.invalid:9");
    env::set_var("AIFO_PROXY_FALLBACK", "0");

    let outcome = aifo_coder::proxy::proxy_connectivity_check_with(|_host, _port| false);

    assert_eq!(outcome, aifo_coder::proxy::ProxyCheckOutcome::Skipped);
    assert_eq!(
        env::var("http_proxy").unwrap_or_default(),
        "http://example.invalid:9"
    );
    assert!(!aifo_coder::proxy::should_force_direct_proxy());

    match prev_toggle {
        Some(v) => env::set_var("AIFO_PROXY_FALLBACK", v),
        None => env::remove_var("AIFO_PROXY_FALLBACK"),
    }
    restore_proxy_env(&prev);
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_proxy_connectivity_skips_when_no_proxy_env_present() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev = stash_proxy_env();
    for k in ["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"] {
        env::remove_var(k);
    }

    let outcome = aifo_coder::proxy::proxy_connectivity_check_with(|_host, _port| false);

    assert_eq!(outcome, aifo_coder::proxy::ProxyCheckOutcome::Skipped);
    assert!(!aifo_coder::proxy::should_force_direct_proxy());

    restore_proxy_env(&prev);
    aifo_coder::proxy::reset_proxy_state_for_tests();
}
