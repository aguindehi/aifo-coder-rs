# Cross-compiling examples for aifo-coder

This directory contains example `.cargo/config.toml` snippets to help cross-compile the Rust launcher for common Linux targets from non-Linux hosts.

Notes:
- Install additional Rust targets with rustup, e.g.:
  rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
- Install appropriate cross-linkers on your host:
  - macOS (Homebrew):
    brew install x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
    or use the osx-cross tap/toolchains if needed.
  - Linux (Debian/Ubuntu):
    sudo apt-get update && sudo apt-get install -y gcc-aarch64-linux-gnu gcc-x86-64-linux-gnu
- Place one of the example config files below as `.cargo/config.toml` at the repository root to activate.

Provided examples:
- .cargo/config.toml.x86_64-unknown-linux-gnu
- .cargo/config.toml.aarch64-unknown-linux-gnu

Makefile integration:
- You can build release archives for specific targets via:
  make RELEASE_TARGETS="x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu" release-for-target
- Or build only Linux (x86_64-unknown-linux-gnu) with:
  make release-for-linux

Static vs dynamic:
- These examples use glibc cross-linkers (dynamic).
- For fully static binaries, consider the musl targets:
  rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
  and use appropriate musl cross-linkers (not included here).
