#![allow(clippy::module_name_repetitions)]
//! Deprecated shim implementation (Phase 2 fused shims).
//!
//! The authoritative shim is the fused Rust binary at `src/bin/aifo-shim.rs`.
//! This target is retained temporarily to avoid breaking builds that still
//! reference it, but it must not be installed or invoked at runtime.
//
// ignore-tidy-linelength

fn main() {
    eprintln!(
        "aifo-shim: deprecated shim entrypoint (use the fused src/bin/aifo-shim.rs binary)"
    );
    std::process::exit(86);
}
