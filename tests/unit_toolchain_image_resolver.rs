use std::env;

#[test]
fn unit_toolchain_default_uses_release_tag_without_overrides() {
    // Disable docker to avoid side effects; resolver should still return a string.
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    // Clear per-kind and global tag overrides
    env::remove_var("RUST_TOOLCHAIN_TAG");
    env::remove_var("NODE_TOOLCHAIN_TAG");
    env::remove_var("CPP_TOOLCHAIN_TAG");
    env::remove_var("AIFO_TOOLCHAIN_TAG");
    env::remove_var("AIFO_TAG");
    env::remove_var("AIFO_RUST_TOOLCHAIN_IMAGE");
    env::remove_var("AIFO_RUST_TOOLCHAIN_VERSION");
    env::remove_var("AIFO_NODE_TOOLCHAIN_IMAGE");
    env::remove_var("AIFO_NODE_TOOLCHAIN_VERSION");

    let ver = env!("CARGO_PKG_VERSION");
    let suffix = format!(":release-{ver}");

    let rust_img = aifo_coder::default_toolchain_image("rust");
    assert!(
        rust_img.ends_with(&suffix),
        "rust default image should end with {suffix}, got {rust_img}"
    );
    let rust_name = rust_img.rsplit('/').next().unwrap_or(&rust_img);
    assert!(
        rust_name.starts_with("aifo-coder-toolchain-rust"),
        "rust default image should be first-party, got {rust_img}"
    );

    let node_img = aifo_coder::default_toolchain_image("node");
    assert!(
        node_img.ends_with(&suffix),
        "node default image should end with {suffix}, got {node_img}"
    );
    let node_name = node_img.rsplit('/').next().unwrap_or(&node_img);
    assert!(
        node_name.starts_with("aifo-coder-toolchain-node"),
        "node default image should be first-party, got {node_img}"
    );

    let cpp_img = aifo_coder::default_toolchain_image("c-cpp");
    assert!(
        cpp_img.ends_with(&suffix),
        "c-cpp default image should end with {suffix}, got {cpp_img}"
    );
    let cpp_name = cpp_img.rsplit('/').next().unwrap_or(&cpp_img);
    assert!(
        cpp_name.starts_with("aifo-coder-toolchain-cpp"),
        "c-cpp default image should be first-party, got {cpp_img}"
    );
}

#[test]
fn unit_toolchain_per_kind_tag_overrides_default_release_tag() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");
    env::remove_var("AIFO_TOOLCHAIN_TAG");
    env::remove_var("AIFO_TAG");

    env::set_var("RUST_TOOLCHAIN_TAG", "rtag");
    let rust_img = aifo_coder::default_toolchain_image("rust");
    assert!(
        rust_img.ends_with(":rtag"),
        "RUST_TOOLCHAIN_TAG should override tag, got {rust_img}"
    );

    env::set_var("NODE_TOOLCHAIN_TAG", "ntag");
    let node_img = aifo_coder::default_toolchain_image("node");
    assert!(
        node_img.ends_with(":ntag"),
        "NODE_TOOLCHAIN_TAG should override tag, got {node_img}"
    );

    env::set_var("CPP_TOOLCHAIN_TAG", "ctag");
    let cpp_img = aifo_coder::default_toolchain_image("c-cpp");
    assert!(
        cpp_img.ends_with(":ctag"),
        "CPP_TOOLCHAIN_TAG should override tag, got {cpp_img}"
    );
}

#[test]
fn unit_toolchain_global_tag_overrides_when_per_kind_missing() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    // Remove per-kind tags, set global toolchain tag
    env::remove_var("RUST_TOOLCHAIN_TAG");
    env::remove_var("NODE_TOOLCHAIN_TAG");
    env::remove_var("CPP_TOOLCHAIN_TAG");
    env::set_var("AIFO_TOOLCHAIN_TAG", "globaltag");
    env::remove_var("AIFO_TAG");

    let rust_img = aifo_coder::default_toolchain_image("rust");
    assert!(
        rust_img.ends_with(":globaltag"),
        "AIFO_TOOLCHAIN_TAG should override rust tag when per-kind missing, got {rust_img}"
    );

    let node_img = aifo_coder::default_toolchain_image("node");
    assert!(
        node_img.ends_with(":globaltag"),
        "AIFO_TOOLCHAIN_TAG should override node tag when per-kind missing, got {node_img}"
    );
}

#[test]
fn unit_toolchain_global_aifo_tag_used_when_no_other_tags() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    env::remove_var("RUST_TOOLCHAIN_TAG");
    env::remove_var("NODE_TOOLCHAIN_TAG");
    env::remove_var("CPP_TOOLCHAIN_TAG");
    env::remove_var("AIFO_TOOLCHAIN_TAG");
    env::set_var("AIFO_TAG", "atagt");

    let rust_img = aifo_coder::default_toolchain_image("rust");
    assert!(
        rust_img.ends_with(":atagt"),
        "AIFO_TAG should override rust tag when no other tags, got {rust_img}"
    );

    let node_img = aifo_coder::default_toolchain_image("node");
    assert!(
        node_img.ends_with(":atagt"),
        "AIFO_TAG should override node tag when no other tags, got {node_img}"
    );
}

#[test]
fn unit_toolchain_image_for_version_uses_exact_version_tag() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    let img = aifo_coder::default_toolchain_image_for_version("rust", "1.2.3");
    assert_eq!(
        img, "aifo-coder-toolchain-rust:1.2.3",
        "default_toolchain_image_for_version must use exact version"
    );

    let img_node = aifo_coder::default_toolchain_image_for_version("node", "22.1");
    assert_eq!(
        img_node, "aifo-coder-toolchain-node:22.1",
        "node versioned image must use exact version"
    );
}

#[test]
fn unit_agent_default_effective_image_uses_release_tag_without_overrides() {
    // Disable docker inside resolver, we only inspect returned string.
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    env::remove_var("AIFO_CODER_AGENT_IMAGE");
    env::remove_var("AIFO_CODER_AGENT_TAG");
    env::remove_var("AIFO_TAG");

    // Use an unqualified agent name; registry::resolve_image will add tag.
    let img = aifo_coder::compute_effective_agent_image_for_run("aifo-coder-codex")
        .expect("compute agent image");
    let ver = env!("CARGO_PKG_VERSION");
    let suffix = format!(":release-{ver}");
    assert!(
        img.ends_with(&suffix),
        "agent effective image should end with {suffix}, got {img}"
    );
}

#[test]
fn unit_agent_image_env_override_wins_over_tag_logic() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    env::set_var("AIFO_CODER_AGENT_IMAGE", "custom/image:123");
    env::remove_var("AIFO_CODER_AGENT_TAG");
    env::remove_var("AIFO_TAG");

    let img = aifo_coder::compute_effective_agent_image_for_run("aifo-coder-codex")
        .expect("compute agent image");
    assert_eq!(
        img, "custom/image:123",
        "AIFO_CODER_AGENT_IMAGE must override any tag logic"
    );
}

#[test]
fn unit_agent_tag_overrides_release_tag_and_global_tag() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    env::remove_var("AIFO_CODER_AGENT_IMAGE");
    env::set_var("AIFO_CODER_AGENT_TAG", "agenttag");
    env::set_var("AIFO_TAG", "globaltag");

    let img = aifo_coder::compute_effective_agent_image_for_run("aifo-coder-codex")
        .expect("compute agent image");
    assert!(
        img.ends_with(":agenttag"),
        "agent-specific tag should win over global AIFO_TAG, got {img}"
    );
}

#[test]
fn unit_agent_global_tag_used_when_no_agent_specific_tag() {
    env::set_var("AIFO_CODER_TEST_DISABLE_DOCKER", "1");

    env::remove_var("AIFO_CODER_AGENT_IMAGE");
    env::remove_var("AIFO_CODER_AGENT_TAG");
    env::set_var("AIFO_TAG", "globaltag2");

    let img = aifo_coder::compute_effective_agent_image_for_run("aifo-coder-codex")
        .expect("compute agent image");
    assert!(
        img.ends_with(":globaltag2"),
        "global AIFO_TAG should set agent tag when no agent-specific overrides, got {img}"
    );
}
