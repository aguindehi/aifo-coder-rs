# Toolchains Guide

This guide explains how to use language toolchains with aifo-coder: sidecar containers, PATH shims, the toolexec proxy, caches, and the optional Linux unix-socket transport.

Contents
- Overview
- Commands and flags
- Linux unix-socket mode
- C/C++ sidecar image (build and publish)
- Caches and cache purge
- Examples
- Notes
- Tests

## Overview

- When you attach toolchains, aifo-coder starts one or more language-specific “sidecar” containers and injects a small shim directory on PATH inside the agent, so tools like cargo, npx, python, gcc, go work transparently via a host-side proxy.
- Sidecars share the same workspace bind mount and use named Docker volumes for caching (cargo, npm, pip, ccache, go) to speed up builds across runs.
- No Docker socket is mounted into containers; AppArmor is used when available.

See also: Fork mode documentation in the README (Fork mode section) and the man page (aifo-coder(1), FORK MODE).

## Commands and flags

- Global agent invocation with toolchains:
  - Use one or more `--toolchain` flags (repeatable).
  - Optional versioned toolchain specs:
    - `--toolchain-spec kind[@version]`, e.g. `rust@1.80` → `rust:1.80-slim`, `node@20` → `node:20-bookworm-slim`
  - Optional image overrides and cache controls:
    - `--toolchain-image KIND=IMAGE`
    - `--no-toolchain-cache`
  - Optional Linux-only unix socket transport:
    - `--toolchain-unix-socket`
  - Optional bootstrap actions:
    - `--toolchain-bootstrap typescript=global` (installs a global `tsc` in the node sidecar if requested)

### Examples

TCP proxy (default; works on macOS, Windows, Linux):
```bash
aifo-coder --toolchain rust aider -- cargo --version
aifo-coder --toolchain node aider -- npx --version
aifo-coder --toolchain python aider -- python -m pip --version
aifo-coder --toolchain c-cpp aider -- cmake --version
```

Versioned specs and bootstrap:
```bash
aifo-coder --toolchain-spec rust@1.80 --toolchain-spec node@20 aider -- cargo --version
aifo-coder --toolchain node --toolchain-bootstrap typescript=global aider -- tsc --version
```

Per-run overrides:
```bash
aifo-coder --toolchain rust --toolchain-image rust=rust:1.80-slim aider -- cargo --help
aifo-coder --toolchain node --no-toolchain-cache aider -- npm ci
```

## Linux unix-socket mode

- On Linux, you can use a unix domain socket for the toolexec proxy instead of TCP.
- Benefits: smaller network surface; no reliance on `host.docker.internal`.

How to enable:
```bash
aifo-coder --toolchain rust --toolchain-unix-socket aider -- cargo --version
```

Under the hood:
- The proxy binds to a unix socket, and the socket directory is mounted into the agent at `/run/aifo`.
- The shim resolves `AIFO_TOOLEEXEC_URL=unix:///run/aifo/toolexec.sock` and connects via UnixStream.

## C/C++ sidecar image (build and publish)

- Default reference: `aifo-cpp-toolchain:latest` (Debian bookworm-slim, build-essential, clang, cmake, ninja, pkg-config, ccache).

Build locally:
```bash
make build-toolchain-cpp
```

Rebuild without cache:
```bash
make rebuild-toolchain-cpp
```

Safe multi-arch publish (never to Docker Hub by default):
- Set a private registry prefix via `REGISTRY` or `AIFO_CODER_REGISTRY_PREFIX`.
```bash
make publish-toolchain-cpp PLATFORMS=linux/amd64,linux/arm64 PUSH=1 REGISTRY=repository.migros.net/
```
- If `REGISTRY` is not provided and `PUSH=1`, an OCI archive is written to `dist/aifo-cpp-toolchain-latest.oci.tar` instead of pushing.

## Caches and cache purge

- Caches are enabled by default and persist across runs via named Docker volumes:
  - Rust: `aifo-cargo-registry`, `aifo-cargo-git`
  - Node: `aifo-npm-cache`
  - Python: `aifo-pip-cache`
  - C/C++: `aifo-ccache`
  - Go: `aifo-go`
- Disable caches for a single run: `--no-toolchain-cache`

Purge caches:
```bash
aifo-coder toolchain-cache-clear
make toolchain-cache-clear
```

## Examples

Dry run to preview Docker commands:
```bash
aifo-coder --verbose --dry-run --toolchain rust aider -- cargo build --release
```

Use toolchain subcommand (Phase 1 behavior) to run commands directly in a sidecar:
```bash
aifo-coder toolchain rust -- cargo --version
aifo-coder toolchain node -- npx --version
aifo-coder toolchain python -- python -m pip --version
aifo-coder toolchain c-cpp -- cmake --version
```

## Notes

- On Linux, when using TCP mode, aifo-coder adds `--add-host=host.docker.internal:host-gateway` to ensure connectivity to the host proxy from inside containers.
- On macOS/Windows, `host.docker.internal` resolves automatically inside Docker Desktop/VMs.

## Tests

- TCP proxy smoke (ignored by default; runs when you pass `-- --ignored`):
```bash
make test-proxy-smoke
```

- Linux-only unix-socket proxy smoke (falls back to TCP on non-Linux):
```bash
make test-proxy-unix
```

- C/C++ sidecar dry-run tests:
```bash
make test-toolchain-cpp
```
