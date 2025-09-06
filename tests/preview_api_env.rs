#[test]
fn test_build_docker_cmd_includes_api_env_mappings() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    // Save and set env
    let old_key = std::env::var("AIFO_API_KEY").ok();
    let old_base = std::env::var("AIFO_API_BASE").ok();
    let old_ver = std::env::var("AIFO_API_VERSION").ok();

    std::env::set_var("AIFO_API_KEY", "k-123");
    std::env::set_var("AIFO_API_BASE", "https://example.invalid/base");
    std::env::set_var("AIFO_API_VERSION", "2024-10-01");

    let args = vec!["--help".to_string()];
    let (_cmd, preview) = aifo_coder::build_docker_cmd("aider", &args, "alpine:3.20", None)
        .expect("build_docker_cmd failed");

    // Expect OpenAI-style envs
    assert!(
        preview.contains("-e OPENAI_API_KEY=k-123"),
        "missing OPENAI_API_KEY mapping: {preview}"
    );
    assert!(
        preview.contains("-e OPENAI_BASE_URL=https://example.invalid/base"),
        "missing OPENAI_BASE_URL mapping: {preview}"
    );
    assert!(
        preview.contains("-e OPENAI_API_BASE=https://example.invalid/base"),
        "missing OPENAI_API_BASE mapping: {preview}"
    );
    assert!(
        preview.contains("-e OPENAI_API_VERSION=2024-10-01"),
        "missing OPENAI_API_VERSION mapping: {preview}"
    );
    assert!(
        preview.contains("-e OPENAI_API_TYPE=azure"),
        "missing OPENAI_API_TYPE mapping: {preview}"
    );

    // Expect Azure-style envs
    assert!(
        preview.contains("-e AZURE_OPENAI_API_KEY=k-123"),
        "missing AZURE_OPENAI_API_KEY mapping: {preview}"
    );
    assert!(
        preview.contains("-e AZURE_API_KEY=k-123"),
        "missing AZURE_API_KEY mapping: {preview}"
    );
    assert!(
        preview.contains("-e AZURE_OPENAI_ENDPOINT=https://example.invalid/base"),
        "missing AZURE_OPENAI_ENDPOINT mapping: {preview}"
    );
    assert!(
        preview.contains("-e AZURE_API_BASE=https://example.invalid/base"),
        "missing AZURE_API_BASE mapping: {preview}"
    );
    assert!(
        preview.contains("-e AZURE_OPENAI_API_VERSION=2024-10-01"),
        "missing AZURE_OPENAI_API_VERSION mapping: {preview}"
    );
    assert!(
        preview.contains("-e AZURE_API_VERSION=2024-10-01"),
        "missing AZURE_API_VERSION mapping: {preview}"
    );

    // Restore env
    if let Some(v) = old_key {
        std::env::set_var("AIFO_API_KEY", v);
    } else {
        std::env::remove_var("AIFO_API_KEY");
    }
    if let Some(v) = old_base {
        std::env::set_var("AIFO_API_BASE", v);
    } else {
        std::env::remove_var("AIFO_API_BASE");
    }
    if let Some(v) = old_ver {
        std::env::set_var("AIFO_API_VERSION", v);
    } else {
        std::env::remove_var("AIFO_API_VERSION");
    }
}
