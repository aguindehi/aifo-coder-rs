#[test]
fn test_purge_volume_list_includes_legacy_npm_and_node_cache() {
    let vols = aifo_coder::toolchain_purge_volume_names();
    assert!(
        vols.contains(&"aifo-node-cache"),
        "expected aifo-node-cache to be purged"
    );
    assert!(
        vols.contains(&"aifo-npm-cache"),
        "expected legacy aifo-npm-cache to be purged"
    );
}
