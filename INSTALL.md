
Prerequisites:
- Docker installed and running
- GNU Make (recommended)
- Optional: Rust toolchain (for building the launcher locally)

Quick install:
```bash
make build
./aifo-coder --help
```

Notes:
- To build only full (fat) images: make build-fat
- To build only slim images: make build-slim
- The wrapper script aifo-coder will try to build the Rust launcher with cargo; if cargo is missing, it can build using Docker.
- Images drop apt and procps by default to reduce surface area. Keep them by passing KEEP_APT=1:
```bash
make KEEP_APT=1 build
```

Troubleshooting:
- Ensure your user can run Docker commands without sudo.
- If builds are slow, Docker may be pulling base layers; subsequent runs are faster.
- To list locally built images:
```bash
make docker-images
```
