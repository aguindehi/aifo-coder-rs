use std::fs;
use std::process::Command;

#[test]
fn test_cache_clear_removes_registry_file_with_xdg_runtime_dir() {
    // Prepare a temp XDG_RUNTIME_DIR and create the cache file
    let td = tempfile::tempdir().expect("tmpdir");
    let base = td.path().to_path_buf();
    let old = std::env::var("XDG_RUNTIME_DIR").ok();
    std::env::set_var("XDG_RUNTIME_DIR", &base);

    let cache = base.join("aifo-coder.regprefix");
    fs::write(&cache, "example.com/").expect("write cache file");
    assert!(cache.exists(), "precondition: cache file must exist");

    // Run CLI
    let bin = env!("CARGO_BIN_EXE_aifo-coder");
    let out = Command::new(bin).arg("cache-clear").output().expect("run cache-clear");
    assert!(out.status.success(), "cache-clear exited non-zero");

    // The cache file should be removed
    assert!(!cache.exists(), "cache file should be removed by cache-clear");

    // Restore env
    if let Some(v) = old { std::env::set_var("XDG_RUNTIME_DIR", v); } else { std::env::remove_var("XDG_RUNTIME_DIR"); }
}
