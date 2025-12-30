use std::env;

#[test]
fn unit_proxy_fallback_marks_force_direct_on_successful_retry() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev_proxy = env::var("http_proxy").ok();
    env::set_var("http_proxy", "http://bad-proxy");

    let mut calls = 0;
    let res = aifo_coder::test_probe_with_proxy_fallback(|clear| {
        calls += 1;
        if clear {
            Some(true)
        } else {
            Some(false)
        }
    });

    assert_eq!(calls, 2, "expected retry with proxy cleared");
    assert_eq!(res, Some((true, true)));
    assert!(aifo_coder::should_force_direct_proxy());

    match prev_proxy {
        Some(v) => env::set_var("http_proxy", v),
        None => env::remove_var("http_proxy"),
    }
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_proxy_fallback_skips_when_disabled() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev_proxy = env::var("http_proxy").ok();
    let prev_toggle = env::var("AIFO_PROXY_FALLBACK").ok();
    env::set_var("http_proxy", "http://bad-proxy");
    env::set_var("AIFO_PROXY_FALLBACK", "0");

    let mut calls = 0;
    let res = aifo_coder::test_probe_with_proxy_fallback(|_clear| {
        calls += 1;
        Some(false)
    });

    assert_eq!(calls, 1, "fallback should not retry when disabled");
    assert_eq!(res, Some((false, false)));
    assert!(!aifo_coder::should_force_direct_proxy());

    match prev_proxy {
        Some(v) => env::set_var("http_proxy", v),
        None => env::remove_var("http_proxy"),
    }
    match prev_toggle {
        Some(v) => env::set_var("AIFO_PROXY_FALLBACK", v),
        None => env::remove_var("AIFO_PROXY_FALLBACK"),
    }
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_proxy_fallback_ignores_when_no_proxy_present() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev_proxy = env::var("http_proxy").ok();
    env::remove_var("http_proxy");
    env::remove_var("HTTP_PROXY");
    env::remove_var("https_proxy");
    env::remove_var("HTTPS_PROXY");

    let mut calls = 0;
    let res = aifo_coder::test_probe_with_proxy_fallback(|clear| {
        calls += 1;
        assert!(
            !clear,
            "should not attempt a proxy-cleared retry when no proxy vars are set"
        );
        Some(false)
    });

    // The wrapper will not retry when no proxy env vars are set.
    assert_eq!(calls, 1);
    assert_eq!(res, Some((false, false)));
    assert!(!aifo_coder::should_force_direct_proxy());

    match prev_proxy {
        Some(v) => env::set_var("http_proxy", v),
        None => env::remove_var("http_proxy"),
    }
    env::remove_var("HTTP_PROXY");
    env::remove_var("https_proxy");
    env::remove_var("HTTPS_PROXY");
    aifo_coder::proxy::reset_proxy_state_for_tests();
}

#[test]
fn unit_proxy_fallback_does_not_force_direct_when_forced_proxy() {
    aifo_coder::proxy::reset_proxy_state_for_tests();
    let prev_proxy = env::var("http_proxy").ok();
    let prev_force = env::var("AIFO_PROXY_FORCE_PROXY").ok();
    env::set_var("http_proxy", "http://bad-proxy");
    env::set_var("AIFO_PROXY_FORCE_PROXY", "1");

    let mut calls = 0;
    let res = aifo_coder::test_probe_with_proxy_fallback(|clear| {
        calls += 1;
        if clear {
            Some(true)
        } else {
            Some(false)
        }
    });

    assert_eq!(calls, 2);
    assert_eq!(res, Some((true, true)));
    assert!(
        !aifo_coder::should_force_direct_proxy(),
        "force-proxy override should prevent direct mode"
    );

    match prev_proxy {
        Some(v) => env::set_var("http_proxy", v),
        None => env::remove_var("http_proxy"),
    }
    match prev_force {
        Some(v) => env::set_var("AIFO_PROXY_FORCE_PROXY", v),
        None => env::remove_var("AIFO_PROXY_FORCE_PROXY"),
    }
    aifo_coder::proxy::reset_proxy_state_for_tests();
}
