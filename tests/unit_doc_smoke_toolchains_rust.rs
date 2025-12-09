use std::fs;
use std::path::Path;

/// Documentation smoke test for Rust toolchain docs.
/// This test is lenient: it skips when docs/TOOLCHAINS.md is missing.
#[test]
fn unit_doc_smoke_toolchains_rust() {
    let p = Path::new("docs/TOOLCHAINS.md");
    if !p.exists() {
        eprintln!("skipping: docs/TOOLCHAINS.md not found");
        return;
    }
    let s = match fs::read_to_string(p) {
        Ok(c) => c,
        Err(e) => {
            panic!("failed to read docs/TOOLCHAINS.md: {}", e);
        }
    };
    // Key phrases/headings expected by the spec
    let needles = [
        "AIFO Rust Toolchain",
        // We only require mention of the key Rust env var names, not exact formatting
        "CARGO_HOME",
        "RUSTUP_HOME",
        "sccache",
        "AIFO_RUST_TOOLCHAIN_IMAGE",
        "ownership initialization",
    ];
    for n in needles {
        assert!(
            s.to_ascii_lowercase().contains(&n.to_ascii_lowercase()),
            "docs/TOOLCHAINS.md is missing expected phrase: '{}'",
            n
        );
    }
}
