#
# ──────────────────────────────────────────────────────────────────────────────────────────────
#  🚀  Welcome to the Migros AI Foundation Coding Agent Wrapper  -  The AIFO Coder Agent    🚀
# ──────────────────────────────────────────────────────────────────────────────────────────────
#  🔒  Secure by Design  |  🌍 Cross-Platform  |  🦀 Powered by Rust  |  🧠 Developed by AIFO
#
#  ✨ Features:
#     - Linux: Docker containers with AppArmor when available; seccomp and cgroup namespaces.
#     - macOS: Docker Desktop/Colima VM isolation; same security features inside the VM.
#     - Windows: Docker Desktop VM; Windows Terminal/PowerShell/Git Bash fork orchestration.
#
#  🔧 Building a safer future for coding automation in Migros Group...
#     - Containerized agents; no privileged mode, no host Docker socket.
#     - AppArmor (Linux) with custom 'aifo-coder' or 'docker-default' when available.
#     - Seccomp and cgroup namespaces as reported by Docker.
#     - Per-pane isolated state for forks.
#     - Language toolchain sidecars (rust, node (ts/deno/bun), python, c/cpp, go) via secure proxy.
#     - Optional unix:// proxy on Linux; host-gateway bridging when needed.
#     - Minimal mounts: project workspace, config files, optional GnuPG keyrings.
# ──────────────────────────────────────────────────────────────────────────────────────────────
#  📜 Written 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of Migros AI Foundation at MGB
# ──────────────────────────────────────────────────────────────────────────────────────────────
#

# Build one image per agent with shared base layers for maximal cache reuse.
IMAGE_PREFIX ?= aifo-coder
TAG ?= latest
RUST_TOOLCHAIN_TAG ?= latest

# Set to 0 to drop apt/procps in final images (local default keeps them; CI overrides to 0)
KEEP_APT ?= 1

# In CI pipelines, override to 0 to slim images
ifeq ($(CI),true)
KEEP_APT := 0
endif

# BuildKit/Buildx configuration
USE_BUILDX ?= 1
PLATFORMS ?=
PUSH ?= 0
CACHE_DIR ?= .buildx-cache

# Nextest arguments
ARGS_NEXTEST ?= --profile ci --no-fail-fast --status-level=fail --hide-progress-bar --cargo-quiet --color=always
NEXTEST_VERSION ?= 0.9.114
CARGO_FLAGS ?= --features otel-otlp

# Optional corporate CA for rust toolchain build and more; if present, pass as BuildKit secret
MIGROS_CA ?= $(HOME)/.certificates/MigrosRootCA2.crt


# OpenTelemetry configuration
# ---------------------------
# Compile-time:
# - Telemetry remains compile-time optional via features:
#     otel      -> tracing + dev exporters (stderr/file)
#     otel-otlp -> otel + OTLP exporter + Tokio runtime
# - Internal builds use CARGO_FLAGS ?= --features otel-otlp so telemetry is compiled in by default.
#
# Runtime enablement (when built with otel/otel-otlp):
# - AIFO_CODER_OTEL:
#     unset                     -> telemetry ENABLED by default
#     "1", "true", "yes"        -> telemetry ENABLED
#     "0", "false", "no", "off" -> telemetry DISABLED (telemetry_init() is a no-op)
#
# OTLP endpoint precedence (traces + metrics):
#  1) OTEL_EXPORTER_OTLP_ENDPOINT (runtime env, non-empty) – highest priority
#  2) AIFO_OTEL_DEFAULT_ENDPOINT (baked in at build time via build.rs)
#  3) Code default "http://localhost:4317" when neither of the above is set
#
# Build-time baked-in default:
# - build.rs reads optional configuration and emits AIFO_OTEL_DEFAULT_ENDPOINT:
#     AIFO_OTEL_ENDPOINT_FILE -> path to a file containing the endpoint URL
#     AIFO_OTEL_ENDPOINT      -> endpoint URL as an env var
#   Example local/internal build:
#     echo "http://alloy-collector-az.service.dev.migros.cloud" > otel-otlp.url
#     AIFO_OTEL_ENDPOINT_FILE=otel-otlp.url make build-launcher
#
# Other runtime overrides (when telemetry is enabled):
#   OTEL_EXPORTER_OTLP_TIMEOUT  -> OTLP export timeout (default 5s)
#   OTEL_BSP_*                  -> batch span processor tuning
#   OTEL_TRACES_SAMPLER         -> sampler (e.g. parentbased_traceidratio)
#   OTEL_TRACES_SAMPLER_ARG     -> sampler argument (e.g. 0.1)
#   AIFO_CODER_TRACING_FMT      -> "1" to install fmt logging layer on stderr (honors RUST_LOG)
#   AIFO_CODER_OTEL_METRICS     -> "1" to enable metrics exporter
#
# Note:
# - Makefile must NOT set AIFO_CODER_OTEL or OTEL_EXPORTER_OTLP_ENDPOINT by default;
#   the Rust binary owns runtime defaults. Env vars can still be set per job or manually if needed.
AIFO_OTEL_ENDPOINT_FILE=otel-otlp.url
export AIFO_OTEL_ENDPOINT_FILE

# How many threads to run?
THREADS_GRCOV ?= $(shell getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)

# Restrict grcov to Rust sources recursively (all .rs files, incl. build.rs, src/**, tests/**)
KEEP_ONLY_GRCOV ?= --keep-only "**/*.rs"

# Nextest niceness
NICENESS_CARGO_NEXTEST =? 0

# Agent build source: [git | release]
AIDER_SOURCE ?= release

# Agent version pins (default: latest). Pin for reproducible releases.
CODEX_VERSION ?= latest
CRUSH_VERSION ?= latest
AIDER_VERSION ?= latest
OPENHANDS_VERSION ?= latest
OPENCODE_VERSION ?= latest

# Agent source refs (git/tag/commit)
PLANDEX_GIT_REF ?= main
AIDER_GIT_REF ?= main

# Agent optional features
WITH_PLAYWRIGHT ?= 1
WITH_MCPM_AIDER ?= 1

# Toolchain tags
RUST_TOOLCHAIN_TAG ?= latest
NODE_TOOLCHAIN_TAG ?= latest
CPP_TOOLCHAIN_TAG ?= latest

# Toolchain base tags
RUST_BASE_TAG ?= 1-slim-bookworm
NODE_BASE_TAG ?= 22-bookworm-slim

# Toolchain repos/images (centralized)
TC_REPO_RUST ?= aifo-coder-toolchain-rust
TC_REPO_NODE ?= aifo-coder-toolchain-node
TC_REPO_CPP  ?= aifo-coder-toolchain-cpp

# Help
.PHONY: help banner
.DEFAULT_GOAL := help

# Colorize help titles (bold colors). Honors NO_COLOR; always color otherwise
COLOR_OK := $(shell sh -c '[ -z "$$NO_COLOR" ] && echo 1 || echo 0')
ifeq ($(COLOR_OK),1)
  C_TITLE := \033[1;38;5;27m
  C_TITLE_UL := \033[4;1;38;5;117m
  C_RESET := \033[0m
else
  C_TITLE :=
  C_TITLE_UL :=
  C_RESET :=
endif
title = @printf '%b\n' "$(C_TITLE)$(1)$(C_RESET)"
title_ul = @printf '%b\n' "$(C_TITLE_UL)$(1)$(C_RESET)"

export IMAGE_PREFIX TAG RUST_TOOLCHAIN_TAG NODE_TOOLCHAIN_TAG
export CPP_TOOLCHAIN_TAG RELEASE_PREFIX RELEASE_POSTFIX
export DOCKER_BUILDKIT ?= 1

# Publish release prefix/postfix
RELEASE_PREFIX ?= release
RELEASE_POSTFIX ?=

banner:
	@echo ""
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"
	@echo " 🚀  Welcome to the Migros AI Foundation Coding Agent Wrapper  -  The AIFO Coder Agent    🚀  "
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"
	@echo " 🔒  Secure by Design  |  🌍 Cross-Platform  |  🦀 Powered by Rust  |  🧠 Developed by AIFO   "
	@echo ""
	@echo " ✨ Features:"
	@echo "    - Linux: Docker containers with AppArmor when available; seccomp and cgroup namespaces."
	@echo "    - macOS: Docker Desktop/Colima VM isolation; same security features inside the VM."
	@echo "    - Windows: Docker Desktop VM; Windows Terminal/PowerShell/Git Bash fork orchestration."
	@echo ""
	@echo " 🔧 Building a safer future for coding automation in Migros Group..."
	@echo "    - Containerized agents; no privileged mode, no host Docker socket."
	@echo "    - AppArmor (Linux) with custom 'aifo-coder' or 'docker-default' when available."
	@echo "    - Seccomp and cgroup namespaces as reported by Docker."
	@echo "    - Per-pane isolated state for forks."
	@echo "    - Language toolchain sidecars (rust, node/ts, python, c/cpp, go) via secure proxy."
	@echo "    - Optional unix:// proxy on Linux; host-gateway bridging when needed."
	@echo "    - Minimal mounts: project workspace, config files, optional GnuPG keyrings."
	@echo ""
	@echo " ⚠️  Guardrail: Local builds should prefer docker buildx/BuildKit (DOCKER_BUILDKIT=1)."
	@echo "    Classic 'docker build' (BuildKit disabled) may fail on Dockerfiles using RUN --mount."
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"
	@echo " 📜 Written 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of Migros AI Foundation at MGB "
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"

help: banner
	@echo ""
	$(call title_ul,Variables:)
	@echo ""
	@echo "  IMAGE_PREFIX  ............... Image name prefix for per-agent images (aifo-coder)"
	@echo "  TAG ......................... Tag for images (default: latest)"
	@echo ""
	@echo "  USE_BUILDX .................. Use docker buildx when available; fallback to docker build (default: 1)"
	@echo "  PLATFORMS ................... Comma-separated platforms for buildx (e.g., linux/amd64,linux/arm64)"
	@echo "  PUSH ........................ With PLATFORMS set, push multi-arch images instead of loading (default: 0)"
	@echo "  REGISTRY .................... Registry prefix for publish (e.g., repository.migros.net/). If unset, we will NOT push."
	@echo "  ADD_ARCH_IN_TAG ............ Append -linux-<arch> to tags for single-arch pushes (default: 1 in CI, 0 locally)"
	@echo "  CACHE_DIR ................... Local buildx cache directory for faster rebuilds (.buildx-cache)"
	@echo "  ARGS ........................ Extra args passed to tests when running 'make test' (e.g., -- --nocapture)"
	@echo "  CLIPPY ...................... Set to 1 to run 'make lint' before 'make test' (default: off)"
	@echo "  DOCKER_BUILDKIT ............. Enable BuildKit for builds (default: 1)"
	@echo "  MIGROS_CA ................... Corporate CA secret path ($(HOME)/.certificates/MigrosRootCA2.crt)"
	@echo "  RUST_TOOLCHAIN_TAG .......... Tag for rust toolchain image (default: latest)"
	@echo "  NODE_TOOLCHAIN_TAG .......... Tag for node toolchain image (default: latest)"
	@echo "  CPP_TOOLCHAIN_TAG ........... Tag for c-cpp toolchain image (default: latest)"
	@echo "  RELEASE_PREFIX .............. Tag prefix for publish-release defaults (default: release)"
	@echo "  RELEASE_POSTFIX ............. Optional suffix for publish-release defaults (e.g., rc1; default: empty)"
	@echo "  RUST_BASE_TAG ............... Base rust image tag (default: 1-slim-bookworm)"
	@echo "  NODE_BASE_TAG ............... Base node image tag (default: 22-bookworm-slim)"
	@echo "  AIFO_CODER_INTERNAL_REGISTRY_PREFIX .. Internal registry to use when REGISTRY unset (optional)"
	@echo "  MIRROR_REGISTRY ............. Mirror for base images reachability (default: repository.migros.net)"
	@echo "  DOCKER_HUB_REGISTRY ......... Docker Hub fallback host (default: registry-1.docker.io)"
	@echo "  WITH_PLAYWRIGHT ............. Install Playwright in Aider images (1=yes, 0=no; default: 1)"
	@echo "  CODEX_VERSION ............... Pin @openai/codex npm version (default: latest)"
	@echo "  CRUSH_VERSION ............... Pin @charmland/crush npm version (default: latest)"
	@echo "  AIDER_VERSION ............... Pin aider-chat pip version in release mode (default: latest)"
	@echo "  AIDER_SOURCE ................ Source for Aider: release (PyPI, default) or git (clone from upstream)"
	@echo "  AIDER_GIT_REF ............... Git ref for Aider when AIDER_SOURCE=git (branch/tag/commit; default: main)"
	@echo "  OPENHANDS_VERSION ........... Pin openhands-ai pip version (default: latest)"
	@echo "  OPENCODE_VERSION ............ Pin opencode-ai npm version (default: latest)"
	@echo "  PLANDEX_GIT_REF ............. Plandex CLI git ref (branch/tag/commit; default: main)"
	@echo "  CRUSH_VERSION ............... Fallback Crush binary version if npm install fails (default: 0.18.4)"
	@echo "  OSX_SDK_FILENAME ............ Apple SDK tarball filename (default: MacOSX.sdk.tar.xz)"
	@echo "  OSXCROSS_REF ................ osxcross Git ref (optional)"
	@echo "  OSXCROSS_SDK_TARBALL ........ Exact Apple SDK tarball name for osxcross (optional)"
	@echo "  APPLE_SDK_URL/BASE64/SHA256 . Sources for Apple SDK in CI (optional)"
	@echo "  AIFO_SKIP_CROSS_BUILD ....... Skip building macOS cross image (default: off)"
	@echo "  RUST_BUILDER_WITH_WIN ....... Include mingw in rust-builder on non-Windows (default: auto)"
	@echo "  COVERAGE_HTML_IMPL .......... Coverage HTML via grcov or genhtml (default: grcov)"
	@echo "  NICENESS_CARGO_NEXTEST ...... niceness for cargo-nextest runs (default: 0)"
	@echo "  AIFO_E2E_MACOS_CROSS ........ Include macOS-cross E2E in acceptance (default: 1)"
	@echo "  AIFO_CODER_TEST_DISABLE_DOCKER .. Disable docker-requiring tests (default: 0)"
	@echo ""
	@echo "  APPARMOR_PROFILE_NAME ....... Rendered AppArmor profile name (default: aifo-coder)"
	@echo "  DIST_DIR .................... Output directory for release archives (dist)"
	@echo "  BIN_NAME .................... Binary name used in release packaging (aifo-coder)"
	@echo "  VERSION ..................... Version inferred from Cargo.toml or git describe"
	@echo "  RELEASE_TARGETS ............. Space-separated Rust targets for 'make release-for-target' (overrides auto-detect)"
	@echo "  CONTAINER ................... Container name for docker-enter (optional)"
	@echo "  CODEX_IMAGE ................. Full image ref for Codex ($${IMAGE_PREFIX}-codex:$${TAG})"
	@echo "  CRUSH_IMAGE ................. Full image ref for Crush ($${IMAGE_PREFIX}-crush:$${TAG})"
	@echo "  AIDER_IMAGE ................. Full image ref for Aider ($${IMAGE_PREFIX}-aider:$${TAG})"
	@echo ""
	@echo "  APP_NAME .................... App bundle name for macOS .app (default: aifo-coder)"
	@echo "  APP_BUNDLE_ID ............... macOS bundle identifier (default: ch.migros.aifo-coder)"
	@echo "  APP_ICON .................... Path to a .icns icon to include in the .app (optional)"
	@echo "  DMG_NAME .................... DMG filename base (default: $${APP_NAME}-$${VERSION})"
	@echo "  SIGN_IDENTITY ............... macOS code signing identity (default: Migros AI Foundation Code Signer)"
	@echo "  NOTARY_PROFILE .............. Keychain profile for xcrun notarytool (optional)"
	@echo "  DMG_BG ...................... Background image for DMG (default: images/aifo-sticker-1024x1024-web.jpg)"
	@echo ""
	$(call title_ul,Install paths (for 'make install'):)
	@echo ""
	@echo "  PREFIX  ..................... Install prefix (/usr/local)"
	@echo "  DESTDIR ..................... Staging root for packaging ()"
	@echo "  BIN_DIR ..................... Binary install dir ($${PREFIX}/bin)"
	@echo "  MAN_DIR ..................... Manpages root ($${PREFIX}/share/man)"
	@echo "  MAN1_DIR .................... Section 1 manpages ($${MAN_DIR}/man1)"
	@echo "  DOC_DIR ..................... Documentation dir ($${PREFIX}/share/doc/$${BIN_NAME})"
	@echo "  EXAMPLES_DIR ................ Examples directory ($${DOC_DIR}/examples)"
	@echo ""
	$(call title_ul,Available Makefile targets:)
	@echo ""
	$(call title,Release and cross-compile:)
	@echo ""
	@echo "  release ..................... Aggregate: build launcher, mac .app + .dmg, and both mac (host) and Linux"
	@echo "  release-for-linux ........... Build Linux release (RELEASE_TARGETS=x86_64-unknown-linux-gnu)"
	@echo "  release-for-mac ............. Build macOS release (RELEASE_TARGETS=aarch64-apple-darwin)"
	@echo "  release-app ................. Build macOS .app bundle into dist/ (Darwin hosts only)"
	@echo "  release-dmg ................. Build macOS .dmg image from the .app (Darwin hosts only)"
	@echo "  release-dmg-sign ............ Sign the .app and .dmg; notarize if configured (Darwin hosts only)"
	@echo "  release-for-target .......... Build release archives into dist/ for targets in RELEASE_TARGETS or host default"
	@echo ""
	@echo "  Hints: set RELEASE_TARGETS to: [x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, aarch64-apple-darwin]"
	@echo ""
	$(call title,Install:)
	@echo ""
	@echo "  install ..................... Install binary, man page, LICENSE/README and examples, then build Docker images locally"
	@echo ""
	$(call title,Build shim:)
	@echo ""
	@echo "  build-shim .................. Build the aifo-shim binary with host toolchain"
	@echo "  build-shim-with-builder ..... Build aifo-shim using the Rust Builder container"
	@echo ""
	$(call title,Build images:)
	@echo ""
	@echo "  build ....................... Build all images"
	@echo ""
	@echo "  build-launcher .............. Build the Rust host launcher (cargo build --release)"
	@echo ""
	@echo "  build-launcher-macos-cross ......... Build aifo-coder for macOS arm64 and x86_64 using cross image"
	@echo "  build-launcher-macos-cross-arm64 ... Build aifo-coder for macOS arm64 using cross image"
	@echo "  build-launcher-macos-cross-x86_64 .. Build aifo-coder for macOS x86_64 using cross image"
	@echo ""
	@echo "  build-coder ................. Build both slim and fat images (all agents)"
	@echo "  build-fat ................... Build all fat images (codex, crush, aider, openhands, opencode, plandex)"
	@echo "  build-slim .................. Build all slim images (codex-slim, crush-slim, aider-slim, openhands-slim, opencode-slim, plandex-slim)"
	@echo ""
	@echo "  build-codex ................. Build only the Codex image ($${IMAGE_PREFIX}-codex:$${TAG})"
	@echo "  build-crush ................. Build only the Crush image ($${IMAGE_PREFIX}-crush:$${TAG})"
	@echo "  build-aider ................. Build only the Aider image ($${IMAGE_PREFIX}-aider:$${TAG})"
	@echo "  build-openhands ............. Build only the OpenHands image ($${IMAGE_PREFIX}-openhands:$${TAG})"
	@echo "  build-opencode .............. Build only the OpenCode image ($${IMAGE_PREFIX}-opencode:$${TAG})"
	@echo "  build-plandex ............... Build only the Plandex image ($${IMAGE_PREFIX}-plandex:$${TAG})"
	@echo ""
	@echo "  build-codex-slim ............ Build only the Codex slim image ($${IMAGE_PREFIX}-codex-slim:$${TAG})"
	@echo "  build-crush-slim ............ Build only the Crush slim image ($${IMAGE_PREFIX}-crush-slim:$${TAG})"
	@echo "  build-aider-slim ............ Build only the Aider slim image ($${IMAGE_PREFIX}-aider-slim:$${TAG})"
	@echo "  build-openhands-slim ........ Build only the OpenHands slim image ($${IMAGE_PREFIX}-openhands-slim:$${TAG})"
	@echo "  build-opencode-slim ......... Build only the OpenCode slim image ($${IMAGE_PREFIX}-opencode-slim:$${TAG})"
	@echo "  build-plandex-slim .......... Build only the Plandex slim image ($${IMAGE_PREFIX}-plandex-slim:$${TAG})"
	@echo ""
	@echo "  build-toolchain ............. Build all toolchain sidecar images (rust/node/cpp)"
	@echo "  build-toolchain-rust ........ Build the Rust toolchain sidecar image ($(TC_REPO_RUST):$(RUST_TOOLCHAIN_TAG))"
	@echo "  build-toolchain-node ........ Build the Node toolchain sidecar image ($(TC_REPO_NODE):$(NODE_TOOLCHAIN_TAG))"
	@echo "  build-toolchain-cpp ......... Build the C-CPP toolchain sidecar image ($(TC_REPO_CPP):$(CPP_TOOLCHAIN_TAG))"
	@echo ""
	@echo "  build-rust-builder .......... Build the Rust cross-compile builder image ($${IMAGE_PREFIX}-rust-builder:$${TAG})"
	@echo "  build-macos-cross-rust-builder Build the macOS cross image (requires ci/osx/$${OSX_SDK_FILENAME})"
	@echo ""
	@echo "  build-debug ................. Debug-build a single Docker stage with buildx and plain logs"
	@echo "                                Use STAGE=codex|crush|aider|*-slim|rust-builder (default: aider) to specify Docker stage"
	@echo ""
	$(call title,Rebuild images:)
	@echo ""
	@echo "  rebuild ..................... Rebuild all images without cache"
	@echo ""
	@echo "  rebuild-coder ............... Rebuild both slim, fat and builder images without cache (all agents)"
	@echo "  rebuild-fat ................. Rebuild all fat images without cache"
	@echo "  rebuild-slim ................ Rebuild all slim images without cache"
	@echo ""
	@echo "  rebuild-codex ............... Rebuild only the Codex image without cache"
	@echo "  rebuild-crush ............... Rebuild only the Crush image without cache"
	@echo "  rebuild-aider ............... Rebuild only the Aider image without cache"
	@echo "  rebuild-openhands ........... Rebuild only the OpenHands image without cache"
	@echo "  rebuild-opencode ............ Rebuild only the OpenCode image without cache"
	@echo "  rebuild-plandex ............. Rebuild only the Plandex image without cache"
	@echo ""
	@echo "  rebuild-codex-slim .......... Rebuild only the Codex slim image without cache"
	@echo "  rebuild-crush-slim .......... Rebuild only the Crush slim image without cache"
	@echo "  rebuild-aider-slim .......... Rebuild only the Aider slim image without cache"
	@echo "  rebuild-openhands-slim ...... Rebuild only the OpenHands slim image without cache"
	@echo "  rebuild-opencode-slim ....... Rebuild only the OpenCode slim image without cache"
	@echo "  rebuild-plandex-slim ........ Rebuild only the Plandex slim image without cache"
	@echo ""
	@echo "  rebuild-toolchain ........... Rebuild all toolchain sidecar images without cache"
	@echo "  rebuild-toolchain-rust ...... Rebuild only the Rust toolchain image without cache"
	@echo "  rebuild-toolchain-node ...... Rebuild only the Node toolchain image without cache"
	@echo "  rebuild-toolchain-cpp ....... Rebuild only the C-CPP toolchain image without cache"
	@echo ""
	@echo "  rebuild-rust-builder ........ Rebuild only the Rust builder image without cache"
	@echo "  rebuild-macos-cross-rust-builder Rebuild the macOS cross image (no cache, pull fresh base)"
	@echo ""
	$(call title,Rebuild existing images by prefix:)
	@echo ""
	@echo "  rebuild-existing ............ Rebuild any existing local images with IMAGE_PREFIX (using cache)"
	@echo "  rebuild-existing-nocache .... Same, but without cache"
	@echo ""
	$(call title,Publish images:)
	@echo ""
	@echo "  publish ..................... Buildx multi-arch and push all images (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo "  publish-release ............. Release wrapper: use defaults (multi-arch, push, tags, cargo version) then run publish"
	@echo ""
	@echo "                                Single-arch CI pushes are tagged with -linux-<arch> suffix to avoid colliding with multi-arch release tags."
	@echo "                                Multi-arch releases keep clean tags. Override behavior with ADD_ARCH_IN_TAG=0 or 1"
	@echo ""
	@echo "                                Note: Set PLATFORMS=linux/amd64,linux/arm64 and PUSH=1 to push multi-arch"
	@echo "                                      with linux/amd64 (Intel); linux/arm64 (Apple Silicon)"
	@echo "                                      Base images support amd64/arm64; use other arches only if upstream supports them."
	@echo "                                Tweak specifics:"
	@echo "                                      make publish-release TAG=release-0.6.4"
	@echo "                                      make publish-release RELEASE_PREFIX=rc"
	@echo "                                      make publish-release RELEASE_POSTFIX=rc1"
	@echo "                                      make publish-release REGISTRY=my.registry/prefix/"
	@echo "                                      make publish-release KEEP_APT=1"
	@echo ""
	@echo "  publish-toolchain-rust ...... Buildx multi-arch and push Rust toolchain (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo "  publish-toolchain-node ...... Buildx multi-arch and push Node toolchain (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo "  publish-toolchain-cpp ....... Buildx multi-arch and push C-CPP toolchain (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo ""
	@echo "  publish-codex ............... Buildx multi-arch and push Codex (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-crush ............... Buildx multi-arch and push Crush (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-aider ............... Buildx multi-arch and push Aider (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-opencode ............ Buildx multi-arch and push OpenCode (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-openhands ........... Buildx multi-arch and push OpenHands (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-plandex ............. Buildx multi-arch and push Plandex (full; set PLATFORMS=... PUSH=1)"
	@echo ""
	@echo "  publish-codex-slim .......... Buildx multi-arch and push Codex (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-crush-slim .......... Buildx multi-arch and push Crush (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-aider-slim .......... Buildx multi-arch and push Aider (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-openhands-slim ...... Buildx multi-arch and push OpenHands (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-opencode-slim ....... Buildx multi-arch and push OpenCode (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-plandex-slim ........ Buildx multi-arch and push Plandex (slim; set PLATFORMS=... PUSH=1)"
	@echo ""
	$(call title,Utilities:)
	@echo ""
	@echo "  clean ....................... Remove built and base images (ignores errors if not present)"
	@echo "  toolchain-cache-clear ....... Purge all toolchain cache Docker volumes (rust/node/npm/pip/ccache/go)"
	@echo "  node-install ................ Host-side pnpm preflight + pnpm install --frozen-lockfile"
	@echo "  node-guard .................. Check for npm/yarn installs touching node_modules (pnpm-only guardrail)"
	@echo "  node-migrate-to-pnpm ........ One-shot migration: npm/yarn → pnpm (removes node_modules + old locks)"
	@echo "  loc ......................... Count lines of source code (Rust, Shell, Dockerfiles, Makefiles, YAML/TOML/JSON, Markdown)"
	@echo "                                Use CONTAINER=name to choose a specific container; default picks first matching prefix."
	@echo "  checksums ................... Generate dist/SHA256SUMS.txt for current artifacts"
	@echo "  sbom ........................ Generate CycloneDX SBOM into dist/SBOM.cdx.json (requires cargo-cyclonedx)"
	@echo ""
	@echo "  docker-images ............... Show the available images in the local Docker registry"
	@echo "  docker-enter ................ Enter a running container via docker exec with GPG runtime prepared"
	@echo "  scrub-coauthors ............. Rewrite history to remove the aider co-author line from all commit messages"
	@echo "                                WARNING: This rewrites history. Ensure you have backups and will force-push."
	@echo ""
	@echo "  gpg-show-config ............. Show current git GPG signing-related configuration"
	@echo "  gpg-enable-signing .......... Re-enable GPG signing for commits and tags in this repo"
	@echo "  gpg-disable-signing ......... Disable GPG signing for commits and tags in this repo (use if commits fail to sign)"
	@echo "  gpg-disable-signing-global .. Disable GPG signing globally (in your ~/.gitconfig)"
	@echo "  gpg-unset-signing ........... Unset local signing config for this repo (return to defaults)"
	@echo ""
	@echo "  git-show-signatures ........ Show commit signature status (git log %h %G? %s)"
	@echo "  git-commit-no-sign .......... Commit staged changes without GPG signing (MESSAGE='your message')"
	@echo "  git-commit-no-sign-all ...... Stage all and commit without signing (MESSAGE='your message' optional)"
	@echo "  git-amend-no-sign ........... Amend the last commit without GPG signing"
	@echo ""
	@echo "  validate-macos-artifact-arm64  Validate macOS arm64 binary with file(1)"
	@echo "  validate-macos-artifact-x86_64 Validate macOS x86_64 binary with file(1)"
	@echo ""
	$(call title,Test targets:)
	@echo ""
	@echo "  lint ........................ Lint by running cargo fmt and cargo clippy (workspace, all targets; -D warnings) and lint naming"
	@echo "  lint-docker ................. Lint by running hadolint on all Dockerfiles of the project"
	@echo "  lint-tests-naming ........... Lint by running lint-test-naming.sh to test files for lane prefixes and conventions"
	@echo "  lint-ultra .................. Lint by running curated clippy: deny unsafe/dbg/await-holding-lock; includes tests by default"
	@echo "                                Set AIFO_ULTRA_INCLUDE_TESTS=0 to skip tests. Set AIFO_ULTRA_WARNINGS=1 to show advisories."
	@echo ""
	@echo "  test ........................ Run Rust tests with cargo-nextest (installs in container if missing)"
	@echo "  test-cargo .................. Run legacy 'cargo test' (no nextest)"
	@echo "  test-legacy ................. Alias for test-cargo"
	@echo "  test-proxy-smoke ............ Run proxy smoke test (ignored by default)"
	@echo "  test-shim-embed ............. Check embedded shim presence in agent image (ignored by default)"
	@echo "  test-proxy-unix ............. Run unix-socket proxy smoke test (ignored by default; Linux-only)"
	@echo "  test-proxy-errors ........... Run proxy error semantics tests (integration)"
	@echo "  test-proxy-tcp .............. Run TCP streaming proxy test (ignored by default)"
	@echo "  test-dev-tool-routing ....... Run dev-tool routing tests (ignored by default)"
	@echo "  test-toolchain-cpp .......... Run c-cpp toolchain dry-run tests"
	@echo "  test-toolchain-rust ......... Run unit/integration rust sidecar tests (exclude ignored/E2E)"
	@echo "  test-toolchain-rust-e2e ..... Run ignored rust sidecar E2E tests (docker required)"
	@echo "  test-macos-cross-image ...... Run macOS cross E2E tests inside cross image (e2e_macos_cross_*)"
	@echo ""
	@echo "  cov ......................... Run coverage-html and coverage-lcov (composite target)"
	@echo "  cov-results ................. Show coverage-html in the browser"
	@echo "  coverage-html ............... Generate HTML coverage via nextest+grcov (rustup/cargo/docker fallback)"
	@echo "  coverage-lcov ............... Generate lcov.info via nextest+grcov (rustup/cargo/docker fallback)"
	@echo ""
	$(call title,Test suites:)
	@echo ""
	@echo "  check ....................... Run 'lint', 'lint-docker', 'lint-tests-naming' then 'test' (lint and unit test suites)"
	@echo "  check-unit .................. Run unit tests (unit test suite)"
	@echo "  check-int ................... Run integration tests (integration test suite)"
	@echo "  check-e2e ................... Run all ignored-by-default tests (acceptance test suite)"
	@echo "  check-all ................... Run all ignored-by-default tests (unit + integration + acceptance suites)"
	@echo ""
	@echo "  test-all-junit .............. Run unit + acceptance + integration in a single nextest run (one JUnit)"
	@echo "  test-acceptance-suite ....... Run acceptance suite (shim/proxy: native HTTP TCP/UDS, wrappers, logs, disconnect, override)"
	@echo "  test-integration-suite ...... Run integration/E2E suite (proxy smoke/unix/errors/tcp, routing, tsc, rust E2E)"
	@echo ""
		$(call title,AppArmor (security) profile:)
	@echo
	@echo "  apparmor .................... Generate build/apparmor/$${APPARMOR_PROFILE_NAME} from template"
	@echo ""
	@echo "  apparmor-load-colima ........ Load the generated profile directly into the Colima VM"
	@echo "  apparmor-log-colima ......... Stream AppArmor logs (Colima VM or local Linux) into build/logs/apparmor.log"
	@echo ""
	$(call title_ul,Usage:)
	@echo ""
	@echo "  Run Aider CLI:"
	@echo "    aifo-coder aider -- [<aider arguments>]"
	@echo ""
	@echo "  Run Codex CLI:"
	@echo "    aifo-coder codex -- [<codex arguments>]"
	@echo ""
	@echo "  Run Crush CLI:"
	@echo "    aifo-coder crush -- [<crush arguments>]"
	@echo ""
	@echo "  Examples:"
	@echo "    aifo-coder aider -- --help"
	@echo "    aifo-coder codex -- --help"
	@echo "    aifo-coder crush -- --help"
	@echo ""
	@echo "    aifo-coder --toolchain rust aider -- --watch-files"
	@echo "    aifo-coder --toolchain node codex -- resume"
	@echo "    aifo-coder --toolchain node crush -- --version"
	@echo ""
	$(call title_ul,Fork mode:)
	@echo ""
	@echo "  aifo-coder --fork N [--fork-include-dirty] [--fork-dissociate] [--fork-session-name NAME]"
	@echo "             [--fork-layout tiled|even-h|even-v] [--fork-keep-on-failure] aider -- [<aider arguments>]"
	@echo "  aifo-coder fork list [--json] [--all-repos]"
	@echo "  aifo-coder fork clean [--session|--older-than|--all] [--dry-run] [--yes] [--keep-dirty|--force] [--json]"
	@echo ""
	@echo "  Variables: AIFO_CODER_FORK_STALE_DAYS to tune stale threshold; AIFO_CODER_FORK_AUTOCLEAN=1 to auto-clean old clean sessions."
	@echo ""
	$(call title_ul,AppArmor:)
	@echo ""
	@echo "   Load AppArmor policy into Colima VM (macOS):"
	@echo "   colima ssh -- sudo apparmor_parser -r -W \"$$PWD/build/apparmor/$${APPARMOR_PROFILE_NAME}\""
	@echo ""
	$(call title_ul,Docs:)
	@echo ""
	@echo "  docs/INSTALL.md ............ Install instructions, prerequisites, targets"
	@echo "  docs/TESTING.md ............ Test lanes, toggles and how to run"
	@echo "  docs/TOOLEXEC.md ........... Shim ↔ proxy HTTP protocol (v1/v2), auth, errors"
	@echo "  docs/CONTRIBUTING.md ....... Toolchain overrides, cache layout, environment notes"
	@echo "  docs/TOOLCHAINS.md ......... Toolchain usage, unix sockets, caches, C/CPP image"
	@echo ""
	$(call title_ul,Tip:)
	@echo ""
	@echo "  Override variables inline, e.g.: make IMAGE_PREFIX=myrepo/aifo-coder TAG=dev build-codex"
	@echo ""

# Detect docker buildx availability
BUILDX_AVAILABLE := $(shell docker buildx version >/dev/null 2>&1 && echo 1 || echo 0)
# Detect buildx driver (e.g., docker, docker-container, containerd)
BUILDX_DRIVER := $(shell docker buildx inspect 2>/dev/null | awk '/^Driver:/{print $$2}')

# Auto-setup a multi-arch buildx builder when PLATFORMS are specified and the current driver is 'docker'.
# This switches to a container-based builder and installs binfmt for amd64/arm64 once on the host.
ifeq ($(USE_BUILDX)$(BUILDX_AVAILABLE),11)
  ifneq ($(strip $(PLATFORMS)),)
    ifeq ($(BUILDX_DRIVER),docker)
      $(info buildx driver 'docker' detected; setting up container-based builder 'aifo' for multi-arch)
      $(shell docker run --privileged --rm tonistiigi/binfmt --install arm64,amd64 >/dev/null 2>&1 || true)
      $(shell docker buildx inspect aifo >/dev/null 2>&1 || docker buildx create --name aifo --driver docker-container --use >/dev/null 2>&1)
      $(shell docker buildx inspect --bootstrap >/dev/null 2>&1 || true)
      BUILDX_DRIVER := $(shell docker buildx inspect 2>/dev/null | awk '/^Driver:/{print $$2}')
    endif
  endif
endif

# Select build command (buildx with cache/load/push or classic docker build)
ifeq ($(USE_BUILDX)$(BUILDX_AVAILABLE),11)
  ifneq ($(strip $(PLATFORMS)),)
    DOCKER_BUILDX_FLAGS := --platform $(PLATFORMS)
    ifeq ($(PUSH),1)
      DOCKER_BUILDX_FLAGS += --push
    endif
  else
    DOCKER_BUILDX_FLAGS := --load
  endif
  ifneq ($(strip $(CACHE_DIR)),)
    ifneq ($(BUILDX_DRIVER),docker)
      DOCKER_BUILDX_FLAGS += --cache-from type=local,src=$(CACHE_DIR) --cache-to type=local,dest=$(CACHE_DIR),mode=max
    else
      $(info buildx driver 'docker' detected; skipping cache export flags)
    endif
  endif
  DOCKER_BUILD = docker buildx build $(DOCKER_BUILDX_FLAGS)
else
  DOCKER_BUILD = docker build
endif

# Coding agent images fat/slim
CODEX_IMAGE ?= $(IMAGE_PREFIX)-codex:$(TAG)
CRUSH_IMAGE ?= $(IMAGE_PREFIX)-crush:$(TAG)
AIDER_IMAGE ?= $(IMAGE_PREFIX)-aider:$(TAG)
OPENHANDS_IMAGE ?= $(IMAGE_PREFIX)-openhands:$(TAG)
OPENCODE_IMAGE ?= $(IMAGE_PREFIX)-opencode:$(TAG)
PLANDEX_IMAGE ?= $(IMAGE_PREFIX)-plandex:$(TAG)

CODEX_IMAGE_SLIM ?= $(IMAGE_PREFIX)-codex-slim:$(TAG)
CRUSH_IMAGE_SLIM ?= $(IMAGE_PREFIX)-crush-slim:$(TAG)
AIDER_IMAGE_SLIM ?= $(IMAGE_PREFIX)-aider-slim:$(TAG)
OPENHANDS_IMAGE_SLIM ?= $(IMAGE_PREFIX)-openhands-slim:$(TAG)
OPENCODE_IMAGE_SLIM ?= $(IMAGE_PREFIX)-opencode-slim:$(TAG)
PLANDEX_IMAGE_SLIM ?= $(IMAGE_PREFIX)-plandex-slim:$(TAG)

RUST_BUILDER_IMAGE ?= $(IMAGE_PREFIX)-rust-builder:$(TAG)
MACOS_CROSS_IMAGE ?= $(IMAGE_PREFIX)-macos-cross-rust-builder:$(TAG)
OSX_SDK_FILENAME ?= MacOSX.sdk.tar.xz

# Include Windows cross toolchain (mingw) automatically on Windows shells
RUST_BUILDER_WITH_WIN ?= 0
UNAME_S := $(shell uname -s 2>/dev/null || echo unknown)
ifeq ($(OS),Windows_NT)
  RUST_BUILDER_WITH_WIN := 1
else ifneq (,$(findstring MINGW,$(UNAME_S))$(findstring MSYS,$(UNAME_S))$(findstring CYGWIN,$(UNAME_S)))
  RUST_BUILDER_WITH_WIN := 1
endif

# Optional corporate CA for rust toolchain build; if present, pass as BuildKit secret
COMMA := ,
RUST_CA_SECRET := $(if $(wildcard $(MIGROS_CA)),--secret id=migros_root_ca$(COMMA)src=$(MIGROS_CA),)
CA_SECRET := $(if $(wildcard $(MIGROS_CA)),--secret id=migros_root_ca$(COMMA)src=$(MIGROS_CA),)

# Append -linux-<arch> to tags for single-arch pushes to avoid collisions with multi-arch releases.
# Default: enabled in CI (CI=true), disabled locally; can be overridden via ADD_ARCH_IN_TAG=0|1.
ADD_ARCH_IN_TAG ?= $(if $(CI),1,0)

# Detect single-platform builds and derive OS/ARCH suffix
ifneq ($(strip $(PLATFORMS)),)
  PLAT_PRIMARY := $(firstword $(subst $(COMMA), ,$(PLATFORMS)))
  PLAT_COUNT := $(words $(subst $(COMMA), ,$(PLATFORMS)))
  OS_FROM_PLAT := $(word 1,$(subst /, ,$(PLAT_PRIMARY)))
  ARCH_FROM_PLAT := $(word 2,$(subst /, ,$(PLAT_PRIMARY)))
  SINGLE_PLAT := $(if $(filter 1,$(PLAT_COUNT)),1,0)
else
  SINGLE_PLAT := 1
  OS_FROM_PLAT := linux
  UNAME_M := $(shell uname -m 2>/dev/null || echo unknown)
  ARCH_FROM_PLAT := $(if $(filter $(UNAME_M),x86_64 amd64),amd64,$(if $(filter $(UNAME_M),aarch64 arm64),arm64,$(UNAME_M)))
endif
ARCH_SUFFIX := $(OS_FROM_PLAT)-$(ARCH_FROM_PLAT)

# Effective tags for registry pushes and OCI archives (local images keep TAG/RUST_TOOLCHAIN_TAG/etc.)
REG_TAG := $(TAG)
ifeq ($(ADD_ARCH_IN_TAG)$(SINGLE_PLAT),11)
  REG_TAG := $(TAG)-$(ARCH_SUFFIX)
endif

RUST_REG_TAG := $(RUST_TOOLCHAIN_TAG)
ifeq ($(ADD_ARCH_IN_TAG)$(SINGLE_PLAT),11)
  RUST_REG_TAG := $(RUST_TOOLCHAIN_TAG)-$(ARCH_SUFFIX)
endif

NODE_REG_TAG := $(NODE_TOOLCHAIN_TAG)
ifeq ($(ADD_ARCH_IN_TAG)$(SINGLE_PLAT),11)
  NODE_REG_TAG := $(NODE_TOOLCHAIN_TAG)-$(ARCH_SUFFIX)
endif

CPP_REG_TAG := $(CPP_TOOLCHAIN_TAG)
ifeq ($(ADD_ARCH_IN_TAG)$(SINGLE_PLAT),11)
  CPP_REG_TAG := $(CPP_TOOLCHAIN_TAG)-$(ARCH_SUFFIX)
endif

# Registry-tagged image names (used only for push/archive); local tags remain unchanged
CODEX_IMAGE_REG ?= $(IMAGE_PREFIX)-codex:$(REG_TAG)
CRUSH_IMAGE_REG ?= $(IMAGE_PREFIX)-crush:$(REG_TAG)
AIDER_IMAGE_REG ?= $(IMAGE_PREFIX)-aider:$(REG_TAG)
OPENHANDS_IMAGE_REG ?= $(IMAGE_PREFIX)-openhands:$(REG_TAG)
OPENCODE_IMAGE_REG ?= $(IMAGE_PREFIX)-opencode:$(REG_TAG)
PLANDEX_IMAGE_REG ?= $(IMAGE_PREFIX)-plandex:$(REG_TAG)
CODEX_IMAGE_SLIM_REG ?= $(IMAGE_PREFIX)-codex-slim:$(REG_TAG)
CRUSH_IMAGE_SLIM_REG ?= $(IMAGE_PREFIX)-crush-slim:$(REG_TAG)
AIDER_IMAGE_SLIM_REG ?= $(IMAGE_PREFIX)-aider-slim:$(REG_TAG)
OPENHANDS_IMAGE_SLIM_REG ?= $(IMAGE_PREFIX)-openhands-slim:$(REG_TAG)
OPENCODE_IMAGE_SLIM_REG ?= $(IMAGE_PREFIX)-opencode-slim:$(REG_TAG)
PLANDEX_IMAGE_SLIM_REG ?= $(IMAGE_PREFIX)-plandex-slim:$(REG_TAG)

TC_IMAGE_RUST ?= $(TC_REPO_RUST):$(if $(strip $(RUST_TOOLCHAIN_TAG)),$(RUST_TOOLCHAIN_TAG),latest)
TC_IMAGE_NODE ?= $(TC_REPO_NODE):$(if $(strip $(NODE_TOOLCHAIN_TAG)),$(NODE_TOOLCHAIN_TAG),latest)
TC_IMAGE_CPP  ?= $(TC_REPO_CPP):$(if $(strip $(CPP_TOOLCHAIN_TAG)),$(CPP_TOOLCHAIN_TAG),latest)

TC_IMAGE_RUST_REG ?= $(TC_REPO_RUST):$(RUST_REG_TAG)
TC_IMAGE_NODE_REG ?= $(TC_REPO_NODE):$(NODE_REG_TAG)
TC_IMAGE_CPP_REG  ?= $(TC_REPO_CPP):$(CPP_REG_TAG)

# Centralized registry reachability and tagging prefix logic
MIRROR_REGISTRY ?= repository.migros.net
DOCKER_HUB_REGISTRY ?= registry-1.docker.io

define MIRROR_CHECK_STRICT
  RP=""; \
  echo "Checking reachability of https://$(MIRROR_REGISTRY) ..."; \
  if command -v curl >/dev/null 2>&1 && \
     curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://$(MIRROR_REGISTRY)/v2/ >/dev/null 2>&1; then \
    echo "$(MIRROR_REGISTRY) reachable via HTTPS; using registry prefix for base images."; RP="$(MIRROR_REGISTRY)/"; \
  else \
    echo "$(MIRROR_REGISTRY) not reachable via HTTPS; using Docker Hub (no prefix)."; \
    if command -v curl >/dev/null 2>&1 && \
       curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://$(DOCKER_HUB_REGISTRY)/v2/ >/dev/null 2>&1; then \
      echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
    else \
      echo "Error: Neither $(MIRROR_REGISTRY) nor Docker Hub is reachable via HTTPS; cannot proceed."; \
      exit 1; \
    fi; \
  fi
endef

define MIRROR_CHECK_LAX
  RP=""; \
  echo "Checking reachability of https://$(MIRROR_REGISTRY) ..."; \
  if command -v curl >/dev/null 2>&1 && \
     curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://$(MIRROR_REGISTRY)/v2/ >/dev/null 2>&1; then \
    echo "$(MIRROR_REGISTRY) reachable via HTTPS; using registry prefix for base images."; RP="$(MIRROR_REGISTRY)/"; \
  else \
    echo "$(MIRROR_REGISTRY) not reachable via HTTPS; proceeding without prefix."; \
  fi
endef

define INTERNAL_REG_SETUP
  REG="$${REGISTRY:-$${AIFO_CODER_INTERNAL_REGISTRY_PREFIX}}"; \
  if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi
endef

define REG_SETUP_WITH_FALLBACK
  REG="$${REGISTRY:-$${AIFO_CODER_INTERNAL_REGISTRY_PREFIX}}"; \
  if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi
endef

.PHONY: build build-coder build-fat build-codex build-crush build-aider build-openhands build-opencode build-plandex build-rust-builder build-launcher
build-fat: build-codex build-crush build-aider build-openhands build-opencode build-plandex

build: build-slim build-fat build-rust-builder build-toolchain build-launcher

build-coder: build-slim build-fat build-rust-builder

build-codex:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) -t "$${REG}$(CODEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) $(CA_SECRET) .; \
	fi

build-crush:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush -t $(CRUSH_IMAGE) -t "$${REG}$(CRUSH_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush -t $(CRUSH_IMAGE) $(CA_SECRET) .; \
	fi

build-aider:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target aider -t $(AIDER_IMAGE) -t "$${REG}$(AIDER_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target aider -t $(AIDER_IMAGE) $(CA_SECRET) .; \
	fi

build-openhands:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands -t $(OPENHANDS_IMAGE) -t "$${REG}$(OPENHANDS_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands -t $(OPENHANDS_IMAGE) $(CA_SECRET) .; \
	fi

build-opencode:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode -t $(OPENCODE_IMAGE) -t "$${REG}$(OPENCODE_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode -t $(OPENCODE_IMAGE) $(CA_SECRET) .; \
	fi

build-plandex:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex -t $(PLANDEX_IMAGE) -t "$${REG}$(PLANDEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex -t $(PLANDEX_IMAGE) $(CA_SECRET) .; \
	fi

build-rust-builder:
	@$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg WITH_WIN="$(RUST_BUILDER_WITH_WIN)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" --target rust-builder -t $(RUST_BUILDER_IMAGE) -t "$${REG}$(RUST_BUILDER_IMAGE)" .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg WITH_WIN="$(RUST_BUILDER_WITH_WIN)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" --target rust-builder -t $(RUST_BUILDER_IMAGE) .; \
	fi

.PHONY: build-macos-cross-rust-builder rebuild-macos-cross-rust-builder build-launcher-macos-cross build-launcher-macos-cross-arm64 build-launcher-macos-cross-x86_64
build-macos-cross-rust-builder:
	@set -e; \
	if [ ! -f "ci/osx/$(OSX_SDK_FILENAME)" ]; then \
	  echo "Error: SDK file 'ci/osx/$(OSX_SDK_FILENAME)' not found. Decode it via ci/bin/decode-apple-sdk.sh or place it locally."; \
	  exit 1; \
	fi; \
	$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) -f builders/macos-cross/Dockerfile --build-arg REGISTRY_PREFIX="$$RP" --build-arg OSX_SDK_FILENAME="$(OSX_SDK_FILENAME)" --target macos-cross-rust-builder -t $(MACOS_CROSS_IMAGE) -t "$${REG}$(MACOS_CROSS_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) -f builders/macos-cross/Dockerfile --build-arg REGISTRY_PREFIX="$$RP" --build-arg OSX_SDK_FILENAME="$(OSX_SDK_FILENAME)" --target macos-cross-rust-builder -t $(MACOS_CROSS_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-macos-cross-rust-builder:
	@set -e; \
	if [ ! -f "ci/osx/$(OSX_SDK_FILENAME)" ]; then \
	  echo "Error: SDK file 'ci/osx/$(OSX_SDK_FILENAME)' not found. Decode it via ci/bin/decode-apple-sdk.sh or place it locally."; \
	  exit 1; \
	fi; \
	$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if docker buildx version >/dev/null 2>&1; then \
	  echo "Rebuilding macOS cross image with buildx (no-cache, pull) ..."; \
	  if [ -n "$(CACHE_DIR)" ] && [ -d "$(CACHE_DIR)" ]; then echo "Purging local buildx cache at $(CACHE_DIR) ..."; rm -rf "$(CACHE_DIR)" || true; fi; \
	  if [ -n "$$REG" ]; then \
	    docker buildx build --no-cache --pull --load \
	      -f builders/macos-cross/Dockerfile \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg OSX_SDK_FILENAME="$(OSX_SDK_FILENAME)" \
	      --target macos-cross-rust-builder \
	      -t $(MACOS_CROSS_IMAGE) \
	      -t "$${REG}$(MACOS_CROSS_IMAGE)" $(CA_SECRET) .; \
	  else \
	    docker buildx build --no-cache --pull --load \
	      -f builders/macos-cross/Dockerfile \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg OSX_SDK_FILENAME="$(OSX_SDK_FILENAME)" \
	      --target macos-cross-rust-builder \
	      -t $(MACOS_CROSS_IMAGE) $(CA_SECRET) .; \
	  fi; \
	else \
	  echo "Rebuilding macOS cross image with classic docker build (no-cache, pull) ..."; \
	  if [ -n "$$REG" ]; then \
	    docker build --no-cache --pull \
	      -f builders/macos-cross/Dockerfile \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg OSX_SDK_FILENAME="$(OSX_SDK_FILENAME)" \
	      --target macos-cross-rust-builder \
	      -t $(MACOS_CROSS_IMAGE) \
	      -t "$${REG}$(MACOS_CROSS_IMAGE)" $(CA_SECRET) .; \
	  else \
	    docker build --no-cache --pull \
	      -f builders/macos-cross/Dockerfile \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg OSX_SDK_FILENAME="$(OSX_SDK_FILENAME)" \
	      --target macos-cross-rust-builder \
	      -t $(MACOS_CROSS_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

build-launcher-macos-cross-arm64:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	echo "Building macOS arm64 launcher inside $(MACOS_CROSS_IMAGE) ..."; \
	MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	  -v "$$PWD:/workspace" \
	  -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	  -v "$$HOME/.cargo/git:/root/.cargo/git" \
	  -v "$$PWD/target:/workspace/target" \
	  $(MACOS_CROSS_IMAGE) sh -lc 'rustup target add aarch64-apple-darwin || true; cargo build --release --target aarch64-apple-darwin'; \
	BIN="target/aarch64-apple-darwin/release/aifo-coder"; \
	if [ -x "$$BIN" ]; then \
	  if command -v file >/dev/null 2>&1; then file "$$BIN" | sed -n "1p"; fi; \
	  echo "Built $$BIN"; \
	else \
	  echo "Error: $$BIN not found or not executable"; \
	  exit 2; \
	fi

# Aggregate: build both arm64 and x86_64
build-launcher-macos-cross: build-launcher-macos-cross-arm64 build-launcher-macos-cross-x86_64

build-launcher-macos-cross-x86_64:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	echo "Building macOS x86_64 launcher inside $(MACOS_CROSS_IMAGE) ..."; \
	MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	  -v "$$PWD:/workspace" \
	  -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	  -v "$$HOME/.cargo/git:/root/.cargo/git" \
	  -v "$$PWD/target:/workspace/target" \
	  $(MACOS_CROSS_IMAGE) sh -lc 'rustup target add x86_64-apple-darwin || true; cargo build --release --target x86_64-apple-darwin'; \
	BIN="target/x86_64-apple-darwin/release/aifo-coder"; \
	if [ -x "$$BIN" ]; then \
	  if command -v file >/dev/null 2>&1; then file "$$BIN" | sed -n "1p"; fi; \
	  echo "Built $$BIN"; \
	else \
	  echo "Error: $$BIN not found or not executable"; \
	  exit 2; \
	fi

.PHONY: validate-macos-artifact-arm64
validate-macos-artifact-arm64:
	@set -e; \
	BIN1="dist/aifo-coder-macos-arm64"; \
	BIN2="target/aarch64-apple-darwin/release/aifo-coder"; \
	if [ -f "$$BIN1" ]; then BIN="$$BIN1"; \
	elif [ -f "$$BIN2" ]; then BIN="$$BIN2"; \
	else \
	  echo "Error: macOS artifact not found at $$BIN1 or $$BIN2"; \
	  exit 2; \
	fi; \
	if command -v file >/dev/null 2>&1; then \
	  out="$$(file "$$BIN" | sed -n "1p")"; echo "$$out"; \
	  echo "$$out" | grep -qi "Mach-O 64-bit arm64" || { \
	    echo "Validation failed: not a Mach-O 64-bit arm64 binary."; exit 3; \
	  }; \
	else \
	  echo "Warning: file(1) not available; skipping validation."; \
	fi; \
	echo "macOS artifact validation OK: $$BIN"

.PHONY: test-macos-cross-image
test-macos-cross-image:
	@set -e; \
	if ! docker image inspect $(MACOS_CROSS_IMAGE) >/dev/null 2>&1; then \
	  echo "Error: $(MACOS_CROSS_IMAGE) not present locally. Hint: make build-macos-cross-rust-builder"; \
	  exit 1; \
	fi; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	echo "Running macOS cross image tests inside $(MACOS_CROSS_IMAGE) ..."; \
	MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	  -v "$$PWD:/workspace" \
	  -v "$$PWD/target:/workspace/target" \
	  $(if $(wildcard $(MIGROS_CA)),-v "$(MIGROS_CA):/run/secrets/migros_root_ca:ro",) \
	  -w /workspace \
	  -t -e TERM=xterm-256color -e CARGO_TERM_COLOR=always \
	  $(MACOS_CROSS_IMAGE) sh -lc 'set -e; CA="/run/secrets/migros_root_ca"; if [ -f "$$CA" ]; then install -m 0644 "$$CA" /usr/local/share/ca-certificates/migros-root-ca.crt || true; command -v update-ca-certificates >/dev/null 2>&1 && update-ca-certificates || true; export SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt; export CARGO_HTTP_CAINFO=/etc/ssl/certs/ca-certificates.crt; export CURL_CA_BUNDLE=/etc/ssl/certs/ca-certificates.crt; fi; export PATH="/usr/local/cargo/bin:/usr/local/rustup/bin:/usr/sbin:/usr/bin:/sbin:/bin:$$PATH"; export RUSTC="/usr/local/cargo/bin/rustc"; unset LD; sccache --version || true; sccache --show-stats || true; rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true; /usr/local/cargo/bin/cargo nextest -V >/dev/null 2>&1 || /usr/local/cargo/bin/cargo install cargo-nextest --locked; export TMPDIR=/var/tmp; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; unset CARGO_TARGET_DIR; /usr/local/cargo/bin/cargo nextest run --target-dir /var/tmp/aifo-target $(ARGS_NEXTEST) --run-ignored ignored-only -E "test(/^e2e_macos_cross_/)"'

.PHONY: validate-macos-artifact-x86_64
validate-macos-artifact-x86_64:
	@set -e; \
	BIN1="dist/aifo-coder-macos-x86_64"; \
	BIN2="target/x86_64-apple-darwin/release/aifo-coder"; \
	if [ -f "$$BIN1" ]; then BIN="$$BIN1"; \
	elif [ -f "$$BIN2" ]; then BIN="$$BIN2"; \
	else \
	  echo "Error: macOS artifact not found at $$BIN1 or $$BIN2"; \
	  exit 2; \
	fi; \
	if command -v file >/dev/null 2>&1; then \
	  out="$$(file "$$BIN" | sed -n "1p")"; echo "$$out"; \
	  echo "$$out" | grep -qi "Mach-O 64-bit x86_64" || { \
	    echo "Validation failed: not a Mach-O 64-bit x86_64 binary."; exit 3; \
	  }; \
	else \
	  echo "Warning: file(1) not available; skipping validation."; \
	fi; \
	echo "macOS artifact validation OK: $$BIN"

.PHONY: ensure-macos-cross-image
ensure-macos-cross-image:
	@set -e; \
	if [ -n "$$AIFO_SKIP_CROSS_BUILD" ]; then \
	  echo "Skipping macOS cross image build (AIFO_SKIP_CROSS_BUILD=1)"; \
	  exit 0; \
	fi; \
	if docker image inspect $(MACOS_CROSS_IMAGE) >/dev/null 2>&1; then \
	  echo "macOS cross image present: $(MACOS_CROSS_IMAGE)"; \
	  exit 0; \
	fi; \
	mkdir -p ci/osx; \
	if [ -f "ci/osx/$(OSX_SDK_FILENAME)" ]; then \
	  echo "SDK found locally, building macOS cross image ..."; \
	  $(MAKE) build-macos-cross-rust-builder; \
	elif [ -n "$$APPLE_SDK_URL" ]; then \
	  echo "Fetching Apple SDK from APPLE_SDK_URL ..."; \
	  if command -v curl >/dev/null 2>&1; then \
	    curl -fL --retry 3 --connect-timeout 10 --max-time 600 "$$APPLE_SDK_URL" -o "ci/osx/$(OSX_SDK_FILENAME)"; \
	    if [ -n "$$APPLE_SDK_SHA256" ]; then \
	      echo "$$APPLE_SDK_SHA256  ci/osx/$(OSX_SDK_FILENAME)" | sha256sum -c -; \
	    fi; \
	    $(MAKE) build-macos-cross-rust-builder; \
	  else \
	    echo "Error: curl not available; cannot fetch APPLE_SDK_URL. Skipping macOS cross." >&2; \
	  fi; \
	elif [ -n "$$APPLE_SDK_BASE64" ]; then \
	  echo "Decoding Apple SDK from APPLE_SDK_BASE64 ..."; \
	  printf '%s' "$$APPLE_SDK_BASE64" | base64 -d > "ci/osx/$(OSX_SDK_FILENAME)"; \
	  $(MAKE) build-macos-cross-rust-builder; \
	else \
	  echo "Skipping macOS cross: SDK not available (provide ci/osx/$(OSX_SDK_FILENAME), APPLE_SDK_URL or APPLE_SDK_BASE64)."; \
	fi

.PHONY: build-debug
build-debug:
	@set -e; \
	STAGE="$${STAGE:-aider}"; \
	echo "Debug building stage '$$STAGE' with docker buildx (plain progress) ..."; \
	$(MIRROR_CHECK_LAX); \
	case "$$STAGE" in \
	  codex) OUT="$(CODEX_IMAGE)" ;; \
	  crush) OUT="$(CRUSH_IMAGE)" ;; \
	  aider) OUT="$(AIDER_IMAGE)" ;; \
	  openhands) OUT="$(OPENHANDS_IMAGE)" ;; \
	  opencode) OUT="$(OPENCODE_IMAGE)" ;; \
	  plandex) OUT="$(PLANDEX_IMAGE)" ;; \
	  codex-slim) OUT="$(CODEX_IMAGE_SLIM)" ;; \
	  crush-slim) OUT="$(CRUSH_IMAGE_SLIM)" ;; \
	  aider-slim) OUT="$(AIDER_IMAGE_SLIM)" ;; \
	  openhands-slim) OUT="$(OPENHANDS_IMAGE_SLIM)" ;; \
	  opencode-slim) OUT="$(OPENCODE_IMAGE_SLIM)" ;; \
	  plandex-slim) OUT="$(PLANDEX_IMAGE_SLIM)" ;; \
	  rust-builder) OUT="$(RUST_BUILDER_IMAGE)" ;; \
	  *) OUT="$(IMAGE_PREFIX)-$$STAGE:$(TAG)" ;; \
	esac; \
	if ! docker buildx version >/dev/null 2>&1; then \
	  echo "Error: docker buildx is not available; please install/enable buildx." >&2; \
	  exit 1; \
	fi; \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  docker buildx build --progress=plain --load \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --target "$$STAGE" \
	    -t "$$OUT" \
	    -t "$${REG}$$OUT" $(CA_SECRET) .; \
	else \
	  docker buildx build --progress=plain --load \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target "$$STAGE" \
	    -t "$$OUT" $(CA_SECRET) .; \
	fi

.PHONY: build-toolchain-rust rebuild-toolchain-rust
build-toolchain-rust:
	@set -e; \
	echo "Building $(TC_IMAGE_RUST) ..."; \
	$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$RP" ]; then echo "Using base image $${RP}rust:$(RUST_BASE_TAG)"; fi; \
	if [ -n "$$REG" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile -t $(TC_IMAGE_RUST) -t "$${REG}$(TC_IMAGE_RUST)" $(RUST_CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile -t $(TC_IMAGE_RUST) $(RUST_CA_SECRET) .; \
	fi

rebuild-toolchain-rust:
	@set -e; \
	echo "Rebuilding $(TC_IMAGE_RUST) (no cache) ..."; \
	$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile -t $(TC_IMAGE_RUST) -t "$${REG}$(TC_IMAGE_RUST)" $(RUST_CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile -t $(TC_IMAGE_RUST) $(RUST_CA_SECRET) .; \
	fi

.PHONY: build-toolchain-node rebuild-toolchain-node
build-toolchain-node:
	@set -e; \
	echo "Building $(TC_IMAGE_NODE) ..."; \
	$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t $(TC_IMAGE_NODE) -t "$${REG}$(TC_IMAGE_NODE)" $(CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t $(TC_IMAGE_NODE) $(CA_SECRET) .; \
	fi

rebuild-toolchain-node:
	@set -e; \
	echo "Rebuilding $(TC_IMAGE_NODE) (no cache) ..."; \
	$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t $(TC_IMAGE_NODE) -t "$${REG}$(TC_IMAGE_NODE)" $(CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t $(TC_IMAGE_NODE) $(CA_SECRET) .; \
	fi

.PHONY: build-toolchain
build-toolchain: build-toolchain-rust build-toolchain-node build-toolchain-cpp

.PHONY: rebuild-toolchain
rebuild-toolchain: rebuild-toolchain-rust rebuild-toolchain-node rebuild-toolchain-cpp

.PHONY: publish-toolchain-rust
publish-toolchain-rust:
	@set -e; \
	echo "Publishing $(TC_IMAGE_RUST) with buildx (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ "$(PUSH)" = "1" ]; then \
	  if [ -n "$$REG" ]; then \
	    echo "PUSH=1 and REGISTRY specified: pushing to $$REG ..."; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile -t "$${REG}$(TC_IMAGE_RUST_REG)" $(RUST_CA_SECRET) .; \
	  else \
	    echo "PUSH=1 but no REGISTRY specified; refusing to push to docker.io. Writing multi-arch OCI archive instead."; \
	    mkdir -p dist; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile --output type=oci,dest=dist/$(TC_REPO_RUST)-$(RUST_REG_TAG).oci.tar $(RUST_CA_SECRET) .; \
	    echo "Wrote dist/$(TC_REPO_RUST)-$(RUST_REG_TAG).oci.tar"; \
	  fi; \
	else \
	  echo "PUSH=0: building locally (single-arch loads into Docker when supported) ..."; \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" -f toolchains/rust/Dockerfile -t $(TC_IMAGE_RUST) $(RUST_CA_SECRET) .; \
	fi

.PHONY: build-toolchain-cpp rebuild-toolchain-cpp


build-toolchain-cpp:
	@set -e; \
	echo "Building $(TC_IMAGE_CPP) ..."; \
	$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t $(TC_IMAGE_CPP) -t "$${REG}$(TC_IMAGE_CPP)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t $(TC_IMAGE_CPP) $(CA_SECRET) .; \
	fi

rebuild-toolchain-cpp:
	@set -e; \
	echo "Rebuilding $(TC_IMAGE_CPP) (no cache) ..."; \
	$(MIRROR_CHECK_STRICT); \
	$(INTERNAL_REG_SETUP); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t $(TC_IMAGE_CPP) -t "$${REG}$(TC_IMAGE_CPP)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t $(TC_IMAGE_CPP) $(CA_SECRET) .; \
	fi

.PHONY: publish-toolchain-cpp

publish-toolchain-cpp:
	@set -e; \
	echo "Publishing $(TC_IMAGE_CPP) with buildx (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ "$(PUSH)" = "1" ]; then \
	  if [ -n "$$REG" ]; then \
	    echo "PUSH=1 and REGISTRY specified: pushing to $$REG ..."; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t "$${REG}$(TC_IMAGE_CPP_REG)" $(CA_SECRET) .; \
	  else \
	    echo "PUSH=1 but no REGISTRY specified; refusing to push to docker.io. Writing multi-arch OCI archive instead."; \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile --output type=oci,dest=dist/$(TC_REPO_CPP)-$(CPP_REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(TC_REPO_CPP)-$(CPP_REG_TAG).oci.tar"; \
	  fi; \
	else \
	  echo "PUSH=0: building locally (single-arch loads into Docker when supported) ..."; \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t $(TC_IMAGE_CPP) $(CA_SECRET) .; \
	fi

.PHONY: publish-toolchain-node
publish-toolchain-node:
	@set -e; \
	echo "Publishing $(TC_IMAGE_NODE) with buildx (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ "$(PUSH)" = "1" ]; then \
	  if [ -n "$$REG" ]; then \
	    echo "PUSH=1 and REGISTRY specified: pushing to $$REG ..."; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t "$${REG}$(TC_IMAGE_NODE_REG)" $(CA_SECRET) .; \
	  else \
	    echo "PUSH=1 but no REGISTRY specified; refusing to push to docker.io. Writing multi-arch OCI archive instead."; \
	    mkdir -p dist; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile --output type=oci,dest=dist/$(TC_REPO_NODE)-$(NODE_REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(TC_REPO_NODE)-$(NODE_REG_TAG).oci.tar"; \
	  fi; \
	else \
	  echo "PUSH=0: building locally (single-arch loads into Docker when supported) ..."; \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t $(TC_IMAGE_NODE) $(CA_SECRET) .; \
	fi

# Publish agent images (full and slim). Tags both local and registry-prefixed refs when REGISTRY is set.
.PHONY: publish-codex publish-codex-slim publish-crush publish-crush-slim publish-aider publish-aider-slim publish-openhands publish-openhands-slim publish-opencode publish-opencode-slim publish-plandex publish-plandex-slim

publish-codex:
	@set -e; \
	echo "Publishing $(CODEX_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t "$${REG}$(CODEX_IMAGE_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex --output type=oci,dest=dist/$(IMAGE_PREFIX)-codex-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-codex-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

publish-codex-slim:
	@set -e; \
	echo "Publishing $(CODEX_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t "$${REG}$(CODEX_IMAGE_SLIM_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim --output type=oci,dest=dist/$(IMAGE_PREFIX)-codex-slim-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-codex-slim-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) $(CA_SECRET) .; \
	  fi; \
	fi

publish-crush:
	@set -e; \
	echo "Publishing $(CRUSH_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush -t "$${REG}$(CRUSH_IMAGE_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush --output type=oci,dest=dist/$(IMAGE_PREFIX)-crush-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-crush-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush -t $(CRUSH_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

publish-crush-slim:
	@set -e; \
	echo "Publishing $(CRUSH_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush-slim -t "$${REG}$(CRUSH_IMAGE_SLIM_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush-slim --output type=oci,dest=dist/$(IMAGE_PREFIX)-crush-slim-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-crush-slim-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) $(CA_SECRET) .; \
	  fi; \
	fi

publish-aider:
	@set -e; \
	echo "Publishing $(AIDER_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target aider -t "$${REG}$(AIDER_IMAGE_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg KEEP_APT="$(KEEP_APT)" \
	      --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	      --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	      --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	      --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	      --target aider --output type=oci,dest=dist/$(IMAGE_PREFIX)-aider-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-aider-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg KEEP_APT="$(KEEP_APT)" \
	      --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	      --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	      --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	      --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	      --target aider -t $(AIDER_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

publish-aider-slim:
	@set -e; \
	echo "Publishing $(AIDER_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target aider-slim -t "$${REG}$(AIDER_IMAGE_SLIM_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg KEEP_APT="$(KEEP_APT)" \
	      --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	      --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	      --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	      --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	      --target aider-slim --output type=oci,dest=dist/$(IMAGE_PREFIX)-aider-slim-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-aider-slim-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) \
	      --build-arg REGISTRY_PREFIX="$$RP" \
	      --build-arg KEEP_APT="$(KEEP_APT)" \
	      --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	      --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	      --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	      --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	      --target aider-slim -t $(AIDER_IMAGE_SLIM) $(CA_SECRET) .; \
	  fi; \
	fi

publish-openhands:
	@set -e; \
	echo "Publishing $(OPENHANDS_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands -t "$${REG}$(OPENHANDS_IMAGE_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands --output type=oci,dest=dist/$(IMAGE_PREFIX)-openhands-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-openhands-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands -t $(OPENHANDS_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

publish-openhands-slim:
	@set -e; \
	echo "Publishing $(OPENHANDS_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands-slim -t "$${REG}$(OPENHANDS_IMAGE_SLIM_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands-slim --output type=oci,dest=dist/$(IMAGE_PREFIX)-openhands-slim-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-openhands-slim-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) $(CA_SECRET) .; \
	  fi; \
	fi

publish-opencode:
	@set -e; \
	echo "Publishing $(OPENCODE_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode -t "$${REG}$(OPENCODE_IMAGE_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode --output type=oci,dest=dist/$(IMAGE_PREFIX)-opencode-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-opencode-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode -t $(OPENCODE_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

publish-opencode-slim:
	@set -e; \
	echo "Publishing $(OPENCODE_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode-slim -t "$${REG}$(OPENCODE_IMAGE_SLIM_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode-slim --output type=oci,dest=dist/$(IMAGE_PREFIX)-opencode-slim-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-opencode-slim-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) $(CA_SECRET) .; \
	  fi; \
	fi

publish-plandex:
	@set -e; \
	echo "Publishing $(PLANDEX_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex -t "$${REG}$(PLANDEX_IMAGE_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex --output type=oci,dest=dist/$(IMAGE_PREFIX)-plandex-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-plandex-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex -t $(PLANDEX_IMAGE) $(CA_SECRET) .; \
	  fi; \
	fi

publish-plandex-slim:
	@set -e; \
	echo "Publishing $(PLANDEX_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	$(INTERNAL_REG_SETUP); \
	$(MIRROR_CHECK_LAX); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex-slim -t "$${REG}$(PLANDEX_IMAGE_SLIM_REG)" $(CA_SECRET) .; \
	else \
	  if [ "$(PUSH)" = "1" ]; then \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex-slim --output type=oci,dest=dist/$(IMAGE_PREFIX)-plandex-slim-$(REG_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/$(IMAGE_PREFIX)-plandex-slim-$(REG_TAG).oci.tar"; \
	  else \
	    $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) $(CA_SECRET) .; \
	  fi; \
	fi

.PHONY: publish
publish:
	@clear
	@echo ""
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"
	@echo "  🚀  Release of the Migros AI Foundation Coding Agent Wrapper  -  The AIFO Coder Agent    🚀 "
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"
	@echo ""
	@echo "VERSION                  : $(VERSION)"
	@echo "RELEASE_PREFIX           : $(RELEASE_PREFIX)"
	@echo "RELEASE_POSTFIX          : $(RELEASE_POSTFIX)"
	@echo "TAG (effective)          : $(TAG)"
	@echo "RUST_TOOLCHAIN_TAG (eff.): $(RUST_TOOLCHAIN_TAG)"
	@echo "NODE_TOOLCHAIN_TAG (eff.): $(NODE_TOOLCHAIN_TAG)"
	@echo "CPP_TOOLCHAIN_TAG (eff.) : $(CPP_TOOLCHAIN_TAG)"
	@echo "PLATFORMS                : $(PLATFORMS)"
	@echo "PUSH                     : $(PUSH)"
	@echo "KEEP_APT                 : $(KEEP_APT)"
	@echo "REGISTRY                 : $(REGISTRY)"
	@echo ""
	@echo "──────────────────────────────────────────────────────────────────────────────────────────────"
	@echo ""
	@read -r -p "Press Enter to continue and push the release (or press ctrl-c to stop) ... " _
	@echo ""
	@$(MAKE) publish-codex
	@$(MAKE) publish-codex-slim
	@$(MAKE) publish-crush
	@$(MAKE) publish-crush-slim
	@$(MAKE) publish-aider
	@$(MAKE) publish-aider-slim
	@$(MAKE) publish-openhands
	@$(MAKE) publish-openhands-slim
	@$(MAKE) publish-opencode
	@$(MAKE) publish-opencode-slim
	@$(MAKE) publish-plandex
	@$(MAKE) publish-plandex-slim
	@$(MAKE) publish-toolchain-rust
	@$(MAKE) publish-toolchain-node
	@$(MAKE) publish-toolchain-cpp

.PHONY: publish-release
publish-release:
	@$(MAKE) \
	  PLATFORMS=$(if $(filter command% environment override,$(origin PLATFORMS)),$(PLATFORMS),linux/amd64$(COMMA)linux/arm64) \
	  PUSH=$(if $(filter command% environment override,$(origin PUSH)),$(PUSH),1) \
	  KEEP_APT=$(if $(filter command% environment override,$(origin KEEP_APT)),$(KEEP_APT),0) \
	  REGISTRY=$(if $(strip $(REGISTRY)),$(REGISTRY),registry.intern.migros.net/ai-foundation/prototypes/aifo-coder-rs/) \
	  RELEASE_PREFIX=$(if $(filter command% environment override,$(origin RELEASE_PREFIX)),$(RELEASE_PREFIX),release) \
	  RELEASE_POSTFIX=$(RELEASE_POSTFIX) \
	  TAG=$(if $(filter command% environment override,$(origin TAG)),$(TAG),$(if $(filter command% environment override,$(origin RELEASE_PREFIX)),$(RELEASE_PREFIX),release)-$(VERSION)$(if $(strip $(RELEASE_POSTFIX)),-$(RELEASE_POSTFIX),)) \
	  RUST_TOOLCHAIN_TAG=$(if $(filter command% environment override,$(origin RUST_TOOLCHAIN_TAG)),$(RUST_TOOLCHAIN_TAG),$(if $(filter command% environment override,$(origin TAG)),$(TAG),$(if $(filter command% environment override,$(origin RELEASE_PREFIX)),$(RELEASE_PREFIX),release)-$(VERSION)$(if $(strip $(RELEASE_POSTFIX)),-$(RELEASE_POSTFIX),))) \
	  NODE_TOOLCHAIN_TAG=$(if $(filter command% environment override,$(origin NODE_TOOLCHAIN_TAG)),$(NODE_TOOLCHAIN_TAG),$(if $(filter command% environment override,$(origin TAG)),$(TAG),$(if $(filter command% environment override,$(origin RELEASE_PREFIX)),$(RELEASE_PREFIX),release)-$(VERSION)$(if $(strip $(RELEASE_POSTFIX)),-$(RELEASE_POSTFIX),))) \
	  CPP_TOOLCHAIN_TAG=$(if $(filter command% environment override,$(origin CPP_TOOLCHAIN_TAG)),$(CPP_TOOLCHAIN_TAG),$(if $(filter command% environment override,$(origin TAG)),$(TAG),$(if $(filter command% environment override,$(origin RELEASE_PREFIX)),$(RELEASE_PREFIX),release)-$(VERSION)$(if $(strip $(RELEASE_POSTFIX)),-$(RELEASE_POSTFIX),))) \
	  publish

.PHONY: build-slim build-codex-slim build-crush-slim build-aider-slim build-openhands-slim build-opencode-slim build-plandex-slim
build-slim: build-codex-slim build-crush-slim build-aider-slim build-openhands-slim build-opencode-slim build-plandex-slim

build-codex-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${REG}$(CODEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg CODEX_VERSION="$(CODEX_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-crush-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${REG}$(CRUSH_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-aider-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${REG}$(AIDER_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --target aider-slim -t $(AIDER_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-openhands-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) -t "$${REG}$(OPENHANDS_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-opencode-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) -t "$${REG}$(OPENCODE_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPCODE_VERSION="$(OPCODE_VERSION)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-plandex-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) -t "$${REG}$(PLANDEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-launcher:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	if [ "$$OS" = "Darwin" ]; then \
	  echo "Building launcher with host Rust toolchain on macOS (container cannot target Apple SDK) ..."; \
	  case "$$ARCH" in \
	    arm64|aarch64) TGT="aarch64-apple-darwin" ;; \
	    x86_64) TGT="x86_64-apple-darwin" ;; \
	    *) TGT="" ;; \
	  esac; \
	  if [ -n "$$TGT" ]; then \
	    if command -v rustup >/dev/null 2>&1; then rustup run stable cargo build $(CARGO_FLAGS) --release --target "$$TGT"; else cargo build $(CARGO_FLAGS) --release --target "$$TGT"; fi; \
	  else \
	    echo "Unsupported macOS architecture: $$ARCH" >&2; exit 1; \
	  fi; \
	else \
	  case "$$OS" in \
	    MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS=""; TGT="x86_64-pc-windows-gnu" ;; \
	    *) case "$$ARCH" in \
	         x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64"; TGT="x86_64-unknown-linux-gnu" ;; \
	         aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64"; TGT="aarch64-unknown-linux-gnu" ;; \
	         *) DOCKER_PLATFORM_ARGS=""; TGT="" ;; \
	       esac ;; \
	  esac; \
	  [ -n "$$TGT" ] || { echo "Unsupported architecture/OS: $$OS $$ARCH" >&2; exit 1; }; \
	  echo "Building launcher inside $(RUST_BUILDER_IMAGE) for target $$TGT ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm -e AIFO_OTEL_ENDPOINT_FILE=/workspace/otel-otlp.url \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) cargo build $(CARGO_FLAGS) --release --target "$$TGT"; \
	fi

.PHONY: build-shim build-shim-with-builder

build-shim:
	@set -e; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest (sidecar) ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(CARGO_FLAGS) $(ARGS_NEXTEST) $(ARGS); \
	  else \
	    echo "cargo-nextest missing in sidecar; attempting prebuilt install ..."; \
	    curl -fsSL --retry 3 --connect-timeout 5 https://get.nexte.st/latest/linux -o /tmp/nextest.tgz 2>/dev/null || true; \
	    if [ -f /tmp/nextest.tgz ]; then mkdir -p /tmp/nextest && tar -C /tmp/nextest -xzf /tmp/nextest.tgz; bin="$$(find /tmp/nextest -type f -name cargo-nextest -print -quit)"; [ -n "$$bin" ] && install -m 0755 "$$bin" /usr/local/cargo/bin/cargo-nextest; rm -rf /tmp/nextest /tmp/nextest.tgz; fi; \
	    if ! cargo nextest -V >/dev/null 2>&1; then arch="$$(uname -m)"; case "$$arch" in x86_64|amd64) tgt="x86_64-unknown-linux-gnu" ;; aarch64|arm64) tgt="aarch64-unknown-linux-gnu" ;; *) tgt="";; esac; if [ -n "$$tgt" ]; then url="https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$(NEXTEST_VERSION)/cargo-nextest-$$tgt.tar.xz"; curl -fsSL --retry 3 --connect-timeout 5 "$$url" -o /tmp/nextest.tar.xz 2>/dev/null && mkdir -p /tmp/nextest && tar -C /tmp/nextest -xf /tmp/nextest.tar.xz && bin="$$(find /tmp/nextest -type f -name cargo-nextest -print -quit)" && [ -n "$$bin" ] && install -m 0755 "$$bin" /usr/local/cargo/bin/cargo-nextest; rm -rf /tmp/nextest /tmp/nextest.tar.xz || true; fi; fi; \
	    cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  fi; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "Building aifo-shim with rustup (stable) ..."; \
	  rustup run stable cargo build $(CARGO_FLAGS) --release --bin aifo-shim; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Building aifo-shim with local cargo ..."; \
	  cargo build $(CARGO_FLAGS) --release --bin aifo-shim; \
	else \
	  echo "Error: cargo not found; use 'make build-shim-with-builder' to build inside Docker." >&2; \
	  exit 1; \
	fi; \
	echo "Built: $$(ls -1 target/*/release/aifo-shim 2>/dev/null || ls -1 target/release/aifo-shim 2>/dev/null || echo 'target/release/aifo-shim')"

build-shim-with-builder:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	echo "Building aifo-shim inside $(RUST_BUILDER_IMAGE) ..."; \
	MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	  -v "$$PWD:/workspace" \
	  -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	  -v "$$HOME/.cargo/git:/root/.cargo/git" \
	  -v "$$PWD/target:/workspace/target" \
	  $(RUST_BUILDER_IMAGE) cargo build $(CARGO_FLAGS) --release --bin aifo-shim; \
	echo "Built (Linux target): $$(ls -1 target/*/release/aifo-shim 2>/dev/null || echo 'target/<triple>/release/aifo-shim')"

.PHONY: lint check check-unit test test-cargo test-legacy coverage coverage-html coverage-lcov coverage-data

lint:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  echo "Running cargo fmt --check (sidecar) ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (sidecar) ..."; \
	  cargo -q clippy --workspace --all-features -- -D warnings; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if [ -n "$$CI" ] || [ "$$AIFO_AUTOINSTALL_COMPONENTS" = "1" ]; then rustup component add --toolchain stable rustfmt clippy >/dev/null 2>&1 || true; fi; \
	  HAVE_FMT=$$(rustup component list --toolchain stable 2>/dev/null | awk '/^rustfmt .* (installed)/{print 1; exit}'); \
	  if [ "$$HAVE_FMT" = "1" ]; then \
	    echo "Using rustup stable rustfmt"; \
	    rustup run stable cargo fmt -- --check || rustup run stable cargo fmt; \
	  else \
	    echo "Using local cargo fmt"; \
	    cargo fmt -- --check || cargo fmt; \
	  fi; \
	  echo "Running cargo clippy (rustup stable) ..."; \
	  HAVE_CLIPPY=$$(rustup component list --toolchain stable 2>/dev/null | awk '/^clippy .* (installed)/{print 1; exit}'); \
	  if [ "$$HAVE_CLIPPY" = "1" ]; then \
	    echo "Using rustup stable clippy (-D warnings)"; \
	    rustup run stable cargo -q clippy --workspace --all-features -- -D warnings; \
	  else \
	    echo "Using local cargo clippy (-D warnings)"; \
	    cargo -q clippy --workspace --all-features -- -D warnings; \
	  fi; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (local cargo) ..."; \
	  cargo -q clippy --workspace --all-features -- -D warnings; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running lint inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; \
	      if cargo fmt --version >/dev/null 2>&1; then cargo fmt -- --check || cargo fmt; else echo "warning: cargo-fmt not installed in builder image; skipping format check" >&2; fi; \
	      cargo -q clippy --workspace --all-features -- -D warnings'; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run lint." >&2; \
	  exit 1; \
	fi

lint-ultra:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	if [ "$${AIFO_ULTRA_WARNINGS:-0}" = "1" ]; then \
	  LINT_FLAGS="-W warnings -W clippy::all -W clippy::pedantic -W clippy::nursery -W clippy::cargo -W clippy::unwrap_used -W clippy::expect_used -W clippy::panic -W clippy::print_stdout -W clippy::print_stderr -W clippy::indexing_slicing"; \
	else \
	  LINT_FLAGS="-A warnings -A clippy::all -A clippy::pedantic -A clippy::nursery -A clippy::cargo -A clippy::unwrap_used -A clippy::expect_used -A clippy::panic -A clippy::print_stdout -A clippy::print_stderr -A clippy::indexing_slicing"; \
	fi; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  echo "Running cargo fmt --check (sidecar) ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (sidecar, curated strict + warnings) ..."; \
	  if [ "$${AIFO_ULTRA_INCLUDE_TESTS:-1}" = "1" ]; then TARGETS="--all-targets"; else TARGETS="--lib --bins"; fi; \
	  cargo -q clippy --workspace --all-features $$TARGETS -- $$LINT_FLAGS -D unsafe_code -D clippy::dbg_macro -D clippy::await_holding_lock; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if [ -n "$$CI" ] || [ "$$AIFO_AUTOINSTALL_COMPONENTS" = "1" ]; then rustup component add --toolchain stable rustfmt clippy >/dev/null 2>&1 || true; fi; \
	  rustup run stable cargo fmt -- --check || rustup run stable cargo fmt || cargo fmt; \
	  echo "Running cargo clippy (rustup stable, curated strict + warnings) ..."; \
	  if rustup run stable cargo -V >/dev/null 2>&1; then USE_RUSTUP=1; else USE_RUSTUP=0; fi; \
	  if [ "$${AIFO_ULTRA_INCLUDE_TESTS:-1}" = "1" ]; then TARGETS="--all-targets"; else TARGETS="--lib --bins"; fi; \
	  if [ "$$USE_RUSTUP" -eq 1 ]; then \
	    rustup run stable cargo -q clippy --workspace --all-features $$TARGETS -- $$LINT_FLAGS -D unsafe_code -D clippy::dbg_macro -D clippy::await_holding_lock; \
	  else \
	    cargo -q clippy --workspace --all-features $$TARGETS -- $$LINT_FLAGS -D unsafe_code -D clippy::dbg_macro -D clippy::await_holding_lock; \
	  fi; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (local cargo, curated strict + warnings) ..."; \
	  if [ "$${AIFO_ULTRA_INCLUDE_TESTS:-1}" = "1" ]; then TARGETS="--all-targets"; else TARGETS="--lib --bins"; fi; \
	  cargo -q clippy --workspace --all-features $$TARGETS -- $$LINT_FLAGS -D unsafe_code -D clippy::dbg_macro -D clippy::await_holding_lock; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running lint inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    -e AIFO_ULTRA_WARNINGS -e AIFO_ULTRA_INCLUDE_TESTS \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; \
	      if cargo fmt --version >/dev/null 2>&1; then cargo fmt -- --check || cargo fmt; else echo "warning: cargo-fmt not installed in builder image; skipping format check" >&2; fi; \
	      if [ "${AIFO_ULTRA_INCLUDE_TESTS:-1}" = "1" ]; then TARGETS="--all-targets"; else TARGETS="--lib --bins"; fi; \
	      cargo -q clippy --workspace --all-features $TARGETS -- $LINT_FLAGS -D unsafe_code -D clippy::dbg_macro -D clippy::await_holding_lock'; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run lint." >&2; \
	  exit 1; \
	fi

check: lint lint-docker lint-tests-naming test

check-unit: test

test:
	@set -e; \
	export AIFO_SHIM_EXIT_ZERO_ON_SIGINT=0; \
	export AIFO_SHIM_EXIT_ZERO_ON_DISCONNECT=0; \
	if [ "$(CLIPPY)" = "1" ]; then $(MAKE) lint; fi; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH$${ARCH:+}" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	echo; \
	echo "Running unit test suite via cargo nextest ..."; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest (sidecar) ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  else \
	    echo "cargo-nextest missing in sidecar; attempting prebuilt install ..."; \
	    curl -fsSL --retry 3 --connect-timeout 5 https://get.nexte.st/latest/linux -o /tmp/nextest.tgz 2>/dev/null || true; \
	    if [ -f /tmp/nextest.tgz ]; then mkdir -p /tmp/nextest && tar -C /tmp/nextest -xzf /tmp/nextest.tgz; bin="$$(find /tmp/nextest -type f -name cargo-nextest -print -quit)"; [ -n "$$bin" ] && install -m 0755 "$$bin" /usr/local/cargo/bin/cargo-nextest; rm -rf /tmp/nextest /tmp/nextest.tgz; fi; \
	    if ! cargo nextest -V >/dev/null 2>&1; then arch="$$(uname -m)"; case "$$(uname -m)" in x86_64|amd64) tgt="x86_64-unknown-linux-gnu" ;; aarch64|arm64) tgt="aarch64-unknown-linux-gnu" ;; *) tgt="";; esac; if [ -n "$$tgt" ]; then url="https://github.com/nextest-rs/nextest/releases/download/cargo-nextest-$(NEXTEST_VERSION)/cargo-nextest-$(NEXTEST_VERSION)-$$tgt.tar.gz"; curl -fsSL --retry 3 --connect-timeout 5 "$$url" -o /tmp/nextest.tgz 2>/dev/null && mkdir -p /tmp/nextest && tar -C /tmp/nextest -xzf /tmp/nextest.tgz && bin="$$(find /tmp/nextest -type f -name cargo-nextest -print -quit)" && [ -n "$$bin" ] && install -m 0755 "$$bin" /usr/local/cargo/bin/cargo-nextest; rm -rf /tmp/nextest /tmp/nextest.tgz || true; fi; fi; \
	    cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  fi; \
	elif command -v rustup >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(CARGO_FLAGS) $(ARGS_NEXTEST) $(ARGS); \
	  elif rustup run stable cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest (rustup stable) ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo nextest run $(CARGO_FLAGS) $(ARGS_NEXTEST) $(ARGS); \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "cargo-nextest not found locally; running inside $(RUST_BUILDER_IMAGE) (first run may install; slower) ..."; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" \
	      -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	      -v "$$HOME/.cargo/git:/root/.cargo/git" \
	      -v "$$PWD/target:/workspace/target" \
	      $(RUST_BUILDER_IMAGE) sh -lc 'set -e; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest run $(CARGO_FLAGS) $(ARGS_NEXTEST) $(ARGS)'; \
	  else \
	    echo "cargo-nextest not available; falling back to cargo test ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo test $(CARGO_FLAGS) $(ARGS); \
	  fi; \
	elif command -v cargo >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "cargo-nextest not found locally; running inside $(RUST_BUILDER_IMAGE) (first run may install; slower) ..."; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" \
	      -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	      -v "$$HOME/.cargo/git:/root/.cargo/git" \
	      -v "$$PWD/target:/workspace/target" \
	      $(RUST_BUILDER_IMAGE) sh -lc 'set -e; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest run $(ARGS_NEXTEST) $(ARGS)'; \
	  else \
	    echo "cargo-nextest not found locally and docker unavailable; running 'cargo test' ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo test $(CARGO_FLAGS) $(ARGS); \
	  fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "cargo/cargo-nextest not found locally; running tests inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(CARGO_FLAGS) $(ARGS_NEXTEST) $(ARGS)'; \
	else \
	  echo "Error: neither cargo-nextest/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

test-cargo:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	if command -v rustup >/dev/null 2>&1; then \
	  echo "Running cargo test locally via rustup (stable toolchain) ..."; \
	  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo test; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Running cargo test locally via cargo ..."; \
	  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo test; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running cargo test inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc 'export CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo test $(CARGO_FLAGS)'; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

test-legacy: test-cargo

define DETECT_PLATFORM_ARGS
OS="$$(uname -s 2>/dev/null || echo unknown)"; \
ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
case "$$OS" in \
  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
  *) case "$$ARCH" in \
       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
       *) DOCKER_PLATFORM_ARGS="" ;; \
     esac ;; \
esac;
endef

define SET_COV_ENV
COV_ENV='CARGO_INCREMENTAL=0 RUSTFLAGS="-C instrument-coverage" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$(PWD)/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0'
endef

define RUN_NEXTEST_WITH_COVERAGE
$(SET_COV_ENV) \
if [ -n "$$AIFO_EXEC_ID" ]; then \
  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
elif command -v rustup >/dev/null 2>&1; then \
  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} rustup run stable cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS) || nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
elif command -v cargo >/dev/null 2>&1; then \
  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} ( cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked ); nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
elif command -v docker >/dev/null 2>&1; then \
  $(DETECT_PLATFORM_ARGS) \
  if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
    echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
    exit 1; \
  fi; \
  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; export CARGO_TARGET_DIR=/var/tmp/aifo-target CARGO_INCREMENTAL=0 RUSTFLAGS="-C instrument-coverage"; export LLVM_PROFILE_FILE=/workspace/build/coverage/aifo-%p-%m.profraw; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)'; \
else \
  echo "error: neither rustup/cargo nor docker found"; \
  exit 1; \
fi
endef

define RUN_GRCOV_LCOV_LOCAL
grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --threads $(THREADS_GRCOV) $(KEEP_ONLY_GRCOV) $(ARGS_GRCOV) $(ARGS) -o build/coverage/lcov.info
endef

define RUN_GRCOV_LCOV_DOCKER
MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
  $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; grcov . --binary-path /var/tmp/aifo-target -s . -t lcov --branch --ignore-not-existing --threads $(THREADS_GRCOV) $(KEEP_ONLY_GRCOV) $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/lcov.info'
endef

define RUN_GRCOV_HTML_LOCAL
grcov . --binary-path target -s . -t html --branch --ignore-not-existing --threads $(THREADS_GRCOV) $(KEEP_ONLY_GRCOV) $(ARGS_GRCOV) $(ARGS) -o build/coverage
endef

define RUN_GRCOV_HTML_DOCKER
MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
  $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; grcov . --binary-path /var/tmp/aifo-target -s . -t html --branch --ignore-not-existing --threads $(THREADS_GRCOV) $(KEEP_ONLY_GRCOV) $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage'
endef

define RESET_HTML_DIR
rm -rf build/coverage/html || true; \
mkdir -p build/coverage/html
endef

define FIX_INDEX_CSS
if [ -f build/coverage/html/index.html ]; then \
  tmp=build/coverage/html/index.html.tmp; \
  sed 's|href="/bulma\.min\.css"|href="./bulma.min.css"|g' build/coverage/html/index.html > "$$tmp" && mv "$$tmp" build/coverage/html/index.html; \
fi
endef

.PHONY: cov coverage-html coverage-lcov coverage-data
cov: coverage-data coverage-lcov coverage-html

coverage-data:
	@set -e; \
	mkdir -p build/coverage; \
	$(RUN_NEXTEST_WITH_COVERAGE); \
	echo "Wrote raw coverage profiles in build/coverage/*.profraw"

cov: coverage-data coverage-lcov coverage-html

coverage-lcov:
	@set -e; \
	mkdir -p build/coverage; \
	if ls build/coverage/*.profraw >/dev/null 2>&1; then \
	  $(DETECT_PLATFORM_ARGS) \
	  if command -v grcov >/dev/null 2>&1; then \
	    $(RUN_GRCOV_LCOV_LOCAL); \
	  elif command -v docker >/dev/null 2>&1; then \
	    $(RUN_GRCOV_LCOV_DOCKER); \
	  else \
	    echo "error: grcov not found and no docker fallback"; \
	    exit 1; \
	  fi; \
	else \
	  $(RUN_NEXTEST_WITH_COVERAGE); \
	  $(DETECT_PLATFORM_ARGS) \
	  if command -v grcov >/dev/null 2>&1; then \
	    $(RUN_GRCOV_LCOV_LOCAL); \
	  elif command -v docker >/dev/null 2>&1; then \
	    $(RUN_GRCOV_LCOV_DOCKER); \
	  else \
	    echo "error: grcov not found and no docker fallback"; \
	    exit 1; \
	  fi; \
	fi; \
	echo "Wrote build/coverage/lcov.info"

coverage-html:
	@set -e; \
	mkdir -p build/coverage; \
	if [ "$${COVERAGE_HTML_IMPL:-}" = "genhtml" ] && [ -f build/coverage/lcov.info ] && command -v genhtml >/dev/null 2>&1; then \
	  $(RESET_HTML_DIR); \
	  genhtml build/coverage/lcov.info --ignore-errors inconsistent,corrupt,range --output-directory build/coverage/html; \
	  $(FIX_INDEX_CSS); \
	  echo "Wrote build/coverage/html from lcov.info via genhtml."; \
	  exit 0; \
	fi; \
	$(DETECT_PLATFORM_ARGS) \
	$(RESET_HTML_DIR); \
	if command -v grcov >/dev/null 2>&1; then \
	  $(RUN_GRCOV_HTML_LOCAL); \
	elif command -v docker >/dev/null 2>&1; then \
	  $(RUN_GRCOV_HTML_DOCKER); \
	else \
	  echo "error: grcov not found and no docker fallback"; \
	  exit 1; \
	fi; \
	$(FIX_INDEX_CSS); \
	echo "Wrote build/coverage/html (grcov HTML)"

.PHONY: test-proxy-smoke test-shim-embed test-proxy-unix test-toolchain-cpp test-proxy-errors
test-proxy-smoke:
	@echo "Running proxy TCP streaming smoke (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test e2e_proxy_streaming_tcp -- --ignored


test-shim-embed:
	@echo "Running embedded shim presence test (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test e2e_shim_embed -- --ignored

test-proxy-unix:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	if [ "$$OS" = "Linux" ]; then \
	  echo "Running unix-socket proxy test (ignored by default; Linux-only) ..."; \
	  CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test e2e_proxy_unix_socket -- --ignored; \
	else \
	  echo "Skipping unix-socket proxy test on $$OS; running TCP proxy smoke instead ..."; \
	  CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test e2e_proxy_streaming_tcp -- --ignored; \
	fi

test-proxy-errors:
	@echo "Running proxy error semantics tests ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test int_proxy_error_semantics

.PHONY: test-proxy-tcp
test-proxy-tcp:
	@echo "Running TCP streaming proxy test (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test e2e_proxy_streaming_tcp -- --ignored

.PHONY: test-acceptance-suite test-integration-suite check-int check-e2e check-all

test-acceptance-suite:
	@set -e; \
	echo "Running acceptance test suite (ignored by default; target-state filters) via cargo nextest ..."; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	if [ "$$OS" = "Linux" ]; then \
	  if [ "$${TEST_E2E_MACOS_CROSS:-1}" = "1" ]; then \
	    EXPR='test(/^e2e_/)' ; \
	  else \
	    EXPR='test(/^e2e_/) & !binary(/^(e2e_macos_cross|e2e_macos_cross_sccache)$$/)' ; \
	  fi; \
	else \
	  if [ "$${AIFO_E2E_MACOS_CROSS:-1}" = "1" ]; then \
	    EXPR='test(/^e2e_/) & !test(/_uds/)' ; \
	  else \
	    EXPR='test(/^e2e_/) & !test(/_uds/) & !binary(/^(e2e_macos_cross|e2e_macos_cross_sccache)$$/)' ; \
	  fi; \
	  echo "Skipping UDS acceptance test (non-Linux host)"; \
	fi; \
	AIFO_CODER_NOTIFICATIONS_TIMEOUT_SECS=5 AIFO_CODER_NOTIFICATIONS_TIMEOUT=5 CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run $(ARGS_NEXTEST) -j 1 --run-ignored ignored-only -E "$$EXPR" $(ARGS); \
	if command -v docker >/dev/null 2>&1 && docker image inspect $(MACOS_CROSS_IMAGE) >/dev/null 2>&1; then \
	  echo "Running macOS cross E2E inside $(MACOS_CROSS_IMAGE) ..."; \
	  $(MAKE) test-macos-cross-image; \
	else \
	  echo "macOS cross image $(MACOS_CROSS_IMAGE) not found locally; skipping macOS cross E2E."; \
	fi

test-integration-suite:
	@set -e; \
	echo "Running integration test suite (target-state filters) via cargo nextest ..."; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	EXPR='test(/^int_/)' ; \
	AIFO_CODER_NOTIFICATIONS_TIMEOUT_SECS=5 AIFO_CODER_NOTIFICATIONS_TIMEOUT=5 CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run $(ARGS_NEXTEST) -j 1 -E "$$EXPR" $(ARGS)

check-e2e:
	@echo "Running ignored-by-default e2e (acceptance) suite ..."
	$(MAKE) test-acceptance-suite

check-int:
	@echo "Running ignored-by-default integration suite ..."
	$(MAKE) test-integration-suite

check-all: check
	@echo "Running full test suite incluing ignored-by-default (unit + integration + all e2e tests) ..."
	$(MAKE) ensure-macos-cross-image
	$(MAKE) test-acceptance-suite
	$(MAKE) test-integration-suite

.PHONY: test-all-junit
test-all-junit:
	@set -e; \
	echo "Running all tests (unit + acceptance + integration) in a single nextest run with JUnit output ..."; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	mkdir -p target/nextest/ci; \
	if [ "$$OS" = "Linux" ]; then \
	  export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0; \
	  if CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest -V >/divert/null 2>&1; then :; else cargo install cargo-nextest --locked; fi; \
	  if [ "${AIFO_CODER_TEST_DISABLE_DOCKER:-0}" = "1" ] || ! command -v docker >/divert/null 2>&1; then \
	    FEX='!test(/^int_/) & !test(/^e2e_/)' ; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS); \
	  else \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run $(ARGS_NEXTEST) --run-ignored all $(ARGS); \
	  fi; \
	else \
	  export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0; \
	  if CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest -V >/dev/null 2>&1; then :; else cargo install cargo-nextest --locked; fi; \
	  if [ "${AIFO_CODER_TEST_DISABLE_DOCKER:-0}" = "1" ] || ! command -v docker >/dev/null 2>&1; then \
	    FEX='!test(/^int_/) & !test(/^e2e_/)' ; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E "$$FEX" $(ARGS); \
	  else \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run $(ARGS_NEXTEST) --run-ignored all -E '!test(/_uds/) & !binary(/^e2e_macos_cross(_sccache)?$$/)' $(ARGS); \
	  fi; \
	fi

.PHONY: test-dev-tool-routing
test-dev-tool-routing:
	@echo "Running dev-tool routing tests (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test e2e_dev_tool_routing_make_tcp_v2 -- --ignored


test-toolchain-cpp:
	@echo "Running c-cpp toolchain dry-run tests ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test int_toolchain_cpp

.PHONY: test-toolchain-rust test-toolchain-rust-e2e
test-toolchain-rust:
	@set -e; \
	if command -v rustup >/dev/null 2>&1; then \
	  rustup run stable cargo nextest -V >/dev/null 2>&1 || rustup run stable cargo install cargo-nextest --locked >/dev/null 2>&1 || true; \
	  echo "Running rust sidecar tests (unit/integration) via nextest ..."; \
	  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo nextest run $(ARGS_NEXTEST) -E 'test(/^int_toolchain_rust_/)' $(ARGS); \
	elif command -v cargo >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running rust sidecar tests (unit/integration) via nextest ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(ARGS_NEXTEST) -E 'test(/^int_toolchain_rust_/)' $(ARGS); \
	  else \
	    echo "Error: cargo-nextest not found; install it with: cargo install cargo-nextest --locked" >&2; exit 1; \
	  fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running rust sidecar tests inside $(RUST_BUILDER_IMAGE) ..."; \
	  OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	  ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	  case "$$OS" in \
	    MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	    *) case "$$ARCH" in \
	         x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	         aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	         *) DOCKER_PLATFORM_ARGS="" ;; \
	       esac ;; \
	  esac; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest run $(ARGS_NEXTEST) -E 'test(/^int_toolchain_rust_/)' $(ARGS)"; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

test-toolchain-rust-e2e:
	@set -e; \
	if command -v rustup >/dev/null 2>&1; then \
	  rustup run stable cargo nextest -V >/dev/null 2>&1 || rustup run stable cargo install cargo-nextest --locked >/dev/null 2>&1 || true; \
	  echo "Running rust sidecar E2E tests (ignored by default) via nextest ..."; \
	  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS); \
	elif command -v cargo >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running rust sidecar E2E tests (ignored by default) via nextest ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS); \
	  else \
	    echo "Error: cargo-nextest not found; install it with: cargo install cargo-nextest --locked" >&2; exit 1; \
	  fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running rust sidecar E2E tests inside $(RUST_BUILDER_IMAGE) ..."; \
	  OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	  ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	  case "$$OS" in \
	    MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	    *) case "$$ARCH" in \
	         x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	         aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	         *) DOCKER_PLATFORM_ARGS="" ;; \
	       esac ;; \
	  esac; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest run $(ARGS_NEXTEST) --run-ignored ignored-only -E 'test(/^e2e_toolchain_rust_/)' $(ARGS)"; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

.PHONY: toolchain-cache-clear
toolchain-cache-clear:
	@echo "Purging toolchain cache volumes (cargo registry/git, node/npm, pip, ccache, go) ..."
	- docker volume rm -f aifo-cargo-registry aifo-cargo-git aifo-node-cache aifo-npm-cache aifo-pip-cache aifo-ccache aifo-go >/dev/null 2>&1 || true
	@echo "Done."

# Host-side Node preflight and install using pnpm and shared .pnpm-store.
# - Creates .pnpm-store with safe permissions when missing
# - Warns if npm/yarn installs are detected
# - Runs pnpm install with frozen lockfile
.PHONY: node-install
node-install:
	@set -e; \
	if ! command -v pnpm >/dev/null 2>&1; then \
	  echo "error: pnpm is required but was not found on PATH."; \
	  echo "       Install with: npm install -g pnpm@9"; \
	  exit 1; \
	fi; \
	if [ -f package-lock.json ]; then \
	  echo "warning: package-lock.json detected; this repository uses pnpm and pnpm-lock.yaml as the"; \
	  echo "         source of truth. Please avoid 'npm install' and use 'pnpm install' instead."; \
	fi; \
	if [ -f yarn.lock ]; then \
	  echo "warning: yarn.lock detected; this repository uses pnpm and pnpm-lock.yaml as the"; \
	  echo "         source of truth. Please avoid 'yarn install' and use 'pnpm install' instead."; \
	fi; \
	if [ ! -d ".pnpm-store" ]; then \
	  echo "Creating .pnpm-store with group-writable permissions ..."; \
	  mkdir -p .pnpm-store; \
	  chmod 775 .pnpm-store || true; \
	fi; \
	if [ -n "$$CI" ]; then \
	  echo "Running pnpm install --frozen-lockfile (CI mode) ..."; \
	else \
	  echo "Running pnpm install --frozen-lockfile ..."; \
	fi; \
	PNPM_STORE_PATH="$$PWD/.pnpm-store" pnpm install --frozen-lockfile

# Simple guardrail to detect npm/yarn installs touching node_modules.
# Intended for local checks and CI preflight (Phase 1 lockfile enforcement).
.PHONY: node-guard
node-guard:
	@set -e; \
	if [ -f package-lock.json ]; then \
	  echo "warning: package-lock.json present; this repository is pnpm-only (pnpm-lock.yaml is canonical)."; \
	fi; \
	if [ -f yarn.lock ]; then \
	  echo "warning: yarn.lock present; this repository is pnpm-only (pnpm-lock.yaml is canonical)."; \
	fi; \
	if [ -d node_modules ] && [ ! -f node_modules/.aifo-node-overlay ]; then \
	  echo "note: node_modules/ exists without overlay sentinel; ensure it was created via pnpm on this host."; \
	fi

# One-shot npm/yarn → pnpm migration helper.
# - Removes node_modules, package-lock.json, yarn.lock (if present)
# - Ensures .pnpm-store exists with safe permissions
# - Runs pnpm install --frozen-lockfile using the shared store
.PHONY: node-migrate-to-pnpm
node-migrate-to-pnpm:
	@set -e; \
	if ! command -v pnpm >/dev/null 2>&1; then \
	  echo "error: pnpm is required but was not found on PATH."; \
	  echo "       Install with: npm install -g pnpm@9"; \
	  exit 1; \
	fi; \
	found_any=0; \
	if [ -f package-lock.json ]; then echo "found: package-lock.json"; found_any=1; fi; \
	if [ -f yarn.lock ]; then echo "found: yarn.lock"; found_any=1; fi; \
	if [ -d node_modules ]; then echo "found: node_modules/"; found_any=1; fi; \
	if [ "$$found_any" -eq 0 ]; then \
	  echo "No npm/yarn artifacts detected (node_modules, package-lock.json, yarn.lock)."; \
	  echo "Nothing to migrate."; \
	  exit 0; \
	fi; \
	if [ -z "$$CI" ]; then \
	  printf "This will REMOVE node_modules/, package-lock.json and yarn.lock (if present), then run pnpm install.\n"; \
	  printf "Continue? [y/N] "; \
	  read ans || ans=""; \
	  case "$$ans" in \
	    y|Y) ;; \
	    *) echo "Aborted."; exit 1;; \
	  esac; \
	else \
	  echo "CI mode: migrating to pnpm without interactive prompt."; \
	fi; \
	if [ -d node_modules ]; then \
	  echo "Removing node_modules/ ..."; \
	  rm -rf node_modules; \
	fi; \
	if [ -f package-lock.json ]; then \
	  echo "Removing package-lock.json ..."; \
	  rm -f package-lock.json; \
	fi; \
	if [ -f yarn.lock ]; then \
	  echo "Removing yarn.lock ..."; \
	  rm -f yarn.lock; \
	fi; \
	if [ ! -d ".pnpm-store" ]; then \
	  echo "Creating .pnpm-store with group-writable permissions ..."; \
	  mkdir -p .pnpm-store; \
	  chmod 775 .pnpm-store || true; \
	fi; \
	echo "Running pnpm install --frozen-lockfile ..."; \
	PNPM_STORE_PATH="$$PWD/.pnpm-store" pnpm install --frozen-lockfile; \
	echo "pnpm migration completed. Commit pnpm-lock.yaml and keep .pnpm-store/ ignored in git."

.PHONY: rebuild rebuild-coder rebuild-fat rebuild-codex rebuild-crush rebuild-aider rebuild-openhands rebuild-opencode rebuild-plandex rebuild-rust-builder
rebuild: rebuild-slim rebuild-fat rebuild-rust-builder rebuild-toolchain

rebuild-coder: rebuild-slim rebuild-fat rebuild-rust-builder

rebuild-fat: rebuild-codex rebuild-crush rebuild-aider rebuild-openhands rebuild-opencode rebuild-plandex

rebuild-codex:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex -t $(CODEX_IMAGE) -t "$${REG}$(CODEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex -t $(CODEX_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-crush:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --no-cache --target crush -t $(CRUSH_IMAGE) -t "$${REG}$(CRUSH_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg CRUSH_VERSION="$(CRUSH_VERSION)" --no-cache --target crush -t $(CRUSH_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-aider:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --no-cache --target aider -t $(AIDER_IMAGE) -t "$${REG}$(AIDER_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --no-cache --target aider -t $(AIDER_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-openhands:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --no-cache --target openhands -t $(OPENHANDS_IMAGE) -t "$${REG}$(OPENHANDS_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --no-cache --target openhands -t $(OPENHANDS_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-opencode:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --no-cache --target opencode -t $(OPENCODE_IMAGE) -t "$${REG}$(OPENCODE_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --no-cache --target opencode -t $(OPENCODE_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-plandex:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --no-cache --target plandex -t $(PLANDEX_IMAGE) -t "$${REG}$(PLANDEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --build-arg PLANDEX_GIT_REF="$(PLANDEX_GIT_REF)" --no-cache --target plandex -t $(PLANDEX_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-rust-builder:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg WITH_WIN="$(RUST_BUILDER_WITH_WIN)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" --target rust-builder -t $(RUST_BUILDER_IMAGE) -t "$${REG}$(RUST_BUILDER_IMAGE)" .; \
	else \
	  $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg WITH_WIN="$(RUST_BUILDER_WITH_WIN)" --build-arg NEXTEST_VERSION="$(NEXTEST_VERSION)" --target rust-builder -t $(RUST_BUILDER_IMAGE) .; \
	fi

.PHONY: rebuild-slim rebuild-codex-slim rebuild-crush-slim rebuild-aider-slim rebuild-openhands-slim rebuild-opencode-slim rebuild-plandex-slim
rebuild-slim: rebuild-codex-slim rebuild-crush-slim rebuild-aider-slim rebuild-openhands-slim rebuild-opencode-slim rebuild-plandex-slim

rebuild-codex-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${REG}$(CODEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex-slim -t $(CODEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-crush-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${REG}$(CRUSH_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush-slim -t $(CRUSH_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-aider-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --no-cache --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${REG}$(AIDER_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) \
	    --build-arg REGISTRY_PREFIX="$$RP" \
	    --build-arg KEEP_APT="$(KEEP_APT)" \
	    --build-arg WITH_PLAYWRIGHT="$(WITH_PLAYWRIGHT)" \
	    --build-arg AIDER_VERSION="$(AIDER_VERSION)" \
	    --build-arg AIDER_SOURCE="$(AIDER_SOURCE)" \
	    --build-arg AIDER_GIT_REF="$(AIDER_GIT_REF)" \
	    --no-cache --target aider-slim -t $(AIDER_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-openhands-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) -t "$${REG}$(OPENHANDS_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg OPENHANDS_VERSION="$(OPENHANDS_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-opencode-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) -t "$${REG}$(OPENCODE_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg OPENCODE_VERSION="$(OPENCODE_VERSION)" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-plandex-slim:
	@$(MIRROR_CHECK_STRICT); \
	$(REG_SETUP_WITH_FALLBACK); \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) -t "$${REG}$(PLANDEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

# Rebuild all existing local images for this prefix (all tags) using cache
.PHONY: rebuild-existing
rebuild-existing:
	@set -e; \
	prefix="$(IMAGE_PREFIX)"; \
	imgs=$$(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E "^$${prefix}-(codex|crush|aider|openhands|opencode|plandex):" || true); \
	if [ -z "$$imgs" ]; then echo "No existing images found for prefix $$prefix"; exit 0; fi; \
	for img in $$imgs; do \
	  repo="$${img%%:*}"; \
	  base="$$(basename "$$repo")"; \
	  agent="$${base##*-}"; \
	  echo "Rebuilding $$img (target=$$agent) ..."; \
	  $(DOCKER_BUILD) --target "$$agent" -t "$$img" .; \
	done

# Rebuild all existing local images for this prefix (all tags) without cache
.PHONY: rebuild-existing-nocache
rebuild-existing-nocache:
	@set -e; \
	prefix="$(IMAGE_PREFIX)"; \
	imgs=$$(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E "^$${prefix}-(codex|crush|aider|openhands|opencode|plandex):" || true); \
	if [ -z "$$imgs" ]; then echo "No existing images found for prefix $$prefix"; exit 0; fi; \
	for img in $$imgs; do \
	  repo="$${img%%:*}"; \
	  base="$$(basename "$$repo")"; \
	  agent="$${base##*-}"; \
	  echo "Rebuilding (no cache) $$img (target=$$agent) ..."; \
	  $(DOCKER_BUILD) --no-cache --target "$$agent" -t "$$img" .; \
	done

.PHONY: clean
clean:
	@set -e; \
	docker rmi $(CODEX_IMAGE) $(CRUSH_IMAGE) $(AIDER_IMAGE) $(OPENHANDS_IMAGE) $(OPENCODE_IMAGE) $(PLANDEX_IMAGE) $(CODEX_IMAGE_SLIM) $(CRUSH_IMAGE_SLIM) $(AIDER_IMAGE_SLIM) $(OPENHANDS_IMAGE_SLIM) $(OPENCODE_IMAGE_SLIM) $(PLANDEX_IMAGE_SLIM) $(RUST_BUILDER_IMAGE) $(TC_IMAGE_RUST) $(TC_IMAGE_NODE) $(TC_IMAGE_CPP) 2>/dev/null || true; \
	docker rmi node:$(NODE_BASE_TAG) rust:$(RUST_BASE_TAG) 2>/dev/null || true; \
	REG="$${REGISTRY:-$${AIFO_CODER_INTERNAL_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	RP="repository.migros.net/"; \
	if [ -n "$$REG" ]; then \
	  docker rmi "$${REG}$(CODEX_IMAGE)" "$${REG}$(CRUSH_IMAGE)" "$${REG}$(AIDER_IMAGE)" "$${REG}$(OPENHANDS_IMAGE)" "$${REG}$(OPENCODE_IMAGE)" "$${REG}$(PLANDEX_IMAGE)" "$${REG}$(CODEX_IMAGE_SLIM)" "$${REG}$(CRUSH_IMAGE_SLIM)" "$${REG}$(AIDER_IMAGE_SLIM)" "$${REG}$(OPENHANDS_IMAGE_SLIM)" "$${REG}$(OPENCODE_IMAGE_SLIM)" "$${REG}$(PLANDEX_IMAGE_SLIM)" "$${REG}$(RUST_BUILDER_IMAGE)" "$${REG}$(TC_IMAGE_RUST)" "$${REG}$(TC_IMAGE_NODE)" "$${REG}$(TC_IMAGE_CPP)" 2>/dev/null || true; \
	  docker rmi "$${REG}node:$(NODE_BASE_TAG)" "$${REG}rust:$(RUST_BASE_TAG)" 2>/dev/null || true; \
	fi; \
	if [ "$$RP" != "$$REG" ]; then \
	  docker rmi "$${RP}$(CODEX_IMAGE)" "$${RP}$(CRUSH_IMAGE)" "$${RP}$(AIDER_IMAGE)" "$${RP}$(OPENHANDS_IMAGE)" "$${RP}$(OPENCODE_IMAGE)" "$${RP}$(PLANDEX_IMAGE)" "$${RP}$(CODEX_IMAGE_SLIM)" "$${RP}$(CRUSH_IMAGE_SLIM)" "$${RP}$(AIDER_IMAGE_SLIM)" "$${RP}$(OPENHANDS_IMAGE_SLIM)" "$${RP}$(OPENCODE_IMAGE_SLIM)" "$${RP}$(PLANDEX_IMAGE_SLIM)" "$${RP}$(RUST_BUILDER_IMAGE)" "$${RP}$(TC_IMAGE_RUST)" "$${RP}$(TC_IMAGE_NODE)" "$${RP}$(TC_IMAGE_CPP)" 2>/dev/null || true; \
	  docker rmi "$${RP}node:$(NODE_BASE_TAG)" "$${RP}rust:$(RUST_BASE_TAG)" 2>/dev/null || true; \
	fi; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	DOCKER_PLATFORM_ARGS=""; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS=""; IS_WIN=1 ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac; IS_WIN=0 ;; \
	esac; \
	if [ "$$IS_WIN" -eq 1 ]; then \
	  if command -v docker >/dev/null 2>&1; then \
	    echo "Running cargo clean inside $(RUST_BUILDER_IMAGE) (Windows) ..."; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" \
	      -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	      -v "$$HOME/.cargo/git:/root/.cargo/git" \
	      -v "$$PWD/target:/workspace/target" \
	      $(RUST_BUILDER_IMAGE) cargo clean; \
	  else \
	    echo "Docker not available; skipping cargo clean on Windows." >&2; \
	  fi; \
	else \
	  if command -v cargo >/dev/null 2>&1; then \
	    cargo clean; \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "cargo not found; running cargo clean inside $(RUST_BUILDER_IMAGE) ..."; \
	    docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" \
	      -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	      -v "$$HOME/.cargo/git:/root/.cargo/git" \
	      -v "$$PWD/target:/workspace/target" \
	      $(RUST_BUILDER_IMAGE) cargo clean; \
	  else \
	    echo "Neither cargo nor docker is available; skipping cargo clean." >&2; \
	  fi; \
	fi

# AppArmor profile generation (for Docker containers)
APPARMOR_PROFILE_NAME ?= aifo-coder

.PHONY: apparmor
ifeq ($(OS),Windows_NT)
apparmor:
	powershell -NoProfile -Command "New-Item -ItemType Directory -Force -Path 'build/apparmor' | Out-Null"
	powershell -NoProfile -Command "(Get-Content 'apparmor/aifo-coder.apparmor.tpl') | ForEach-Object { $_ -replace '__PROFILE_NAME__','$(APPARMOR_PROFILE_NAME)' } | Set-Content 'build/apparmor/$(APPARMOR_PROFILE_NAME)'"
	@echo "Wrote build/apparmor/$(APPARMOR_PROFILE_NAME)"
	@echo "Load into AppArmor on a Linux host with:"
	@echo "  sudo apparmor_parser -r -W build/apparmor/$(APPARMOR_PROFILE_NAME)"
	@echo "Load into Colima's VM (macOS) with:"
	@echo "  colima ssh -- sudo apparmor_parser -r -W \"$(PWD)/build/apparmor/$(APPARMOR_PROFILE_NAME)\""
else
apparmor:
	mkdir -p build/apparmor
	sed -e 's/__PROFILE_NAME__/$(APPARMOR_PROFILE_NAME)/g' apparmor/aifo-coder.apparmor.tpl > build/apparmor/$(APPARMOR_PROFILE_NAME)
	@echo "Wrote build/apparmor/$(APPARMOR_PROFILE_NAME)"
	@echo "Load into AppArmor on a Linux host with:"
	@echo "  sudo apparmor_parser -r -W build/apparmor/$(APPARMOR_PROFILE_NAME)"
	@echo "Load into Colima's VM (macOS) with:"
	@echo "  colima ssh -- sudo apparmor_parser -r -W \"$(PWD)/build/apparmor/$(APPARMOR_PROFILE_NAME)\""
endif

.PHONY: apparmor-load-colima
apparmor-load-colima:
	colima ssh -- sudo apparmor_parser -r -W "$(PWD)/build/apparmor/$(APPARMOR_PROFILE_NAME)"

.PHONY: apparmor-log-colima
apparmor-log-colima:
	@mkdir -p build/logs
	@echo "Streaming AppArmor logs to build/logs/apparmor.log (Ctrl-C to stop)..."
	@if command -v colima >/dev/null 2>&1 && colima ssh -- uname -s >/dev/null 2>&1; then \
		echo "Detected Colima VM; streaming kernel logs from inside the VM..."; \
		colima ssh -- sudo sh -lc 'if command -v journalctl >/dev/null 2>&1; then SYSTEMD_COLORS=1 journalctl -k -n 100 -g apparmor -f --no-tail -o short-iso; elif [ -f /var/log/kern.log ]; then tail -n0 -F /var/log/kern.log | env GREP_COLOR="1;31" GREP_COLORS="ms=1;31" grep -i --color=always -E "apparmor|$$"; elif [ -f /var/log/syslog ]; then tail -n0 -F /var/log/syslog | env GREP_COLOR="1;31" GREP_COLORS="ms=1;31" grep -i --color=always -E "apparmor|$$"; elif command -v dmesg >/dev/null 2>&1; then dmesg -w | env GREP_COLOR="1;31" GREP_COLORS="ms=1;31" grep -i --color=always -E "apparmor|$$"; else echo "No kernel log source available"; fi' \
		| tee -a build/logs/apparmor.log; \
	elif command -v journalctl >/dev/null 2>&1; then \
		echo "Detected local Linux host; streaming kernel logs..."; \
		sudo env SYSTEMD_COLORS=1 journalctl -k -n 100 -g apparmor -f --no-tail -o short-iso | tee -a build/logs/apparmor.log; \
	elif [ -f /var/log/kern.log ]; then \
		tail -n0 -F /var/log/kern.log | env GREP_COLOR="1;31" GREP_COLORS="ms=1;31" grep -i --color=always -E 'apparmor|$$' | tee -a build/logs/apparmor.log; \
	elif [ -f /var/log/syslog ]; then \
		tail -n0 -F /var/log/syslog | env GREP_COLOR="1;31" GREP_COLORS="ms=1;31" grep -i --color=always -E 'apparmor|$$' | tee -a build/logs/apparmor.log; \
	else \
		echo "Unable to locate AppArmor logs. On macOS, ensure Colima is running; on Linux, ensure journalctl/syslog available." >&2; \
		exit 1; \
	fi

.PHONY: docker-images
docker-images:
	@set -e; \
	docker images | head -1; docker images | sort | grep -v REPOSITORY

.PHONY: docker-enter
docker-enter:
	@set -e; \
	cont="$(CONTAINER)"; \
	if [ -z "$$cont" ]; then \
	  prefix="$(IMAGE_PREFIX)"; \
	  cont="$$(docker ps --format '{{.Names}}' --filter "name=^$${prefix}-" | head -n1)"; \
	fi; \
	if [ -z "$$cont" ]; then \
	  echo "No running container found. Provide CONTAINER=name or ensure a container with name prefix '$${prefix:-$(IMAGE_PREFIX)}-' is running."; \
	  exit 1; \
	fi; \
	echo "Entering $$cont ..."; \
	docker exec -it "$$cont" /bin/sh -lc "set -e; \
	  export HOME=\$$HOME; [ -n \"\$$HOME\" ] || export HOME=/home/coder; \
	  [ -n \"\$$GNUPGHOME\" ] || export GNUPGHOME=\"\$$HOME/.gnupg\"; \
	  mkdir -p \"\$$GNUPGHOME\"; chmod 700 \"\$$GNUPGHOME\" || true; \
	  [ -n \"\$$XDG_RUNTIME_DIR\" ] || export XDG_RUNTIME_DIR=\"/tmp/runtime-\`id -u\`\"; \
	  mkdir -p \"\$$XDG_RUNTIME_DIR/gnupg\"; chmod 700 \"\$$XDG_RUNTIME_DIR\" \"\$$XDG_RUNTIME_DIR/gnupg\" || true; \
	  if [ -t 0 ]; then export GPG_TTY=\"/dev/tty\"; fi; \
	  touch \"\$$GNUPGHOME/gpg-agent.conf\"; \
	  sed -i \"/^pinentry-program /d\" \"\$$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || true; \
	  echo \"pinentry-program /usr/bin/pinentry-curses\" >> \"\$$GNUPGHOME/gpg-agent.conf\"; \
	  grep -q \"^allow-loopback-pinentry\" \"\$$GNUPGHOME/gpg-agent.conf\" 2>/dev/null || echo \"allow-loopback-pinentry\" >> \"\$$GNUPGHOME/gpg-agent.conf\"; \
	  gpgconf --kill gpg-agent >/dev/null 2>&1 || true; \
	  gpgconf --launch gpg-agent >/dev/null 2>&1 || true; \
	  exec bash -l || exec sh -l"


.PHONY: gpg-disable-signing gpg-enable-signing gpg-show-config
gpg-disable-signing:
	git config commit.gpgsign false
	git config tag.gpgSign false
	@echo "Disabled GPG signing in local repo: commit.gpgsign=false, tag.gpgSign=false"

gpg-enable-signing:
	git config commit.gpgsign true
	git config tag.gpgSign true
	@echo "Enabled GPG signing in local repo: commit.gpgsign=true, tag.gpgSign=true"

gpg-show-config:
	@echo "commit.gpgsign=$$(git config --get commit.gpgsign || echo unset)"
	@echo "tag.gpgSign=$$(git config --get tag.gpgSign || echo unset)"
	@echo "user.signingkey=$$(git config --get user.signingkey || echo unset)"
	@echo "gpg.program=$$(git config --get gpg.program || echo unset)"

.PHONY: git-show-signatures
git-show-signatures:
	@printf '%s\n' \
'G – A good (valid) signature.' \
'B – A bad signature.' \
'U – A good signature with an untrusted key.' \
'X – A good signature with an expired key.' \
'Y – A good signature with an expired signature.' \
'R – A good signature made by a revoked key.' \
'E – An error occurred during signature verification.' \
'N – No signature.'
	@echo ""
	@git log --pretty=format:'%h %G? %s'

.PHONY: gpg-disable-signing-global gpg-unset-signing git-commit-no-sign git-amend-no-sign git-commit-no-sign-all
gpg-disable-signing-global:
	git config --global commit.gpgsign false
	git config --global tag.gpgSign false
	@echo "Disabled GPG signing globally: commit.gpgsign=false, tag.gpgSign=false"

gpg-unset-signing:
	-@git config --unset commit.gpgsign 2>/dev/null || true
	-@git config --unset commit.gpgSign 2>/dev/null || true
	-@git config --unset tag.gpgSign 2>/dev/null || true
	@echo "Unset local signing config for this repo."

git-commit-no-sign:
	@test -n "$(MESSAGE)" || { echo "Usage: make git-commit-no-sign MESSAGE='your commit message'"; exit 1; }
	git -c commit.gpgsign=false commit -m "$(MESSAGE)" --no-verify

git-amend-no-sign:
	git -c commit.gpgsign=false commit --amend --no-edit --no-verify

git-commit-no-sign-all:
	@msg="$(MESSAGE)"; if [ -z "$$msg" ]; then msg="chore: commit (no-sign) via Makefile"; fi; git add -A && git -c commit.gpgsign=false commit -m "$$msg" --no-verify

.PHONY: scrub-coauthors
scrub-coauthors:
	@echo "About to rewrite git history to remove Co-authored-by: aider (azure/mgb-aifo-model-gpt-5) <aider@aider.chat>"
	@echo "Ensure you have a backup and are prepared to force-push updated branches."
	@command -v git >/dev/null 2>&1 || { echo "git is required"; exit 1; }
	@git --version >/dev/null 2>&1 || { echo "git not working"; exit 1; }
	@git filter-repo -h >/dev/null 2>&1 || { echo "git-filter-repo is required. See https://github.com/newren/git-filter-repo#installation"; exit 1; }
	@git -c commit.gpgsign=false -c tag.gpgSign=false filter-repo --force --message-callback 'import re; return re.sub(br"(?mi)^[ \t]*Co-authored-by: aider \(azure/mgb-aifo-model-gpt-5\) <aider@aider.chat>[ \t]*\r?\n?", b"", message)'

# Release packaging variables
DIST_DIR ?= dist
BIN_NAME ?= aifo-coder
# Cross-platform extraction of version/name without relying on sed on Windows
ifeq ($(OS),Windows_NT)
  POWERSHELL := powershell
  CARGO_VERSION_CMD := $(POWERSHELL) -NoProfile -Command '(Get-Content "Cargo.toml") | ForEach-Object { if($$_ -match '\''^\s*version\s*=\s*"(.*)"'\'' ){ $$matches[1] } } | Select-Object -First 1'
  CARGO_NAME_CMD := $(POWERSHELL) -NoProfile -Command '(Get-Content "Cargo.toml") | ForEach-Object { if($$_ -match '\''^\s*name\s*=\s*"(.*)"'\'' ){ $$matches[1] } } | Select-Object -First 1'
else
  CARGO_VERSION_CMD := awk 'BEGIN{p=0} /^\[package\]/{p=1;next} /^\[/{p=0} p && $$0 ~ /^[[:space:]]*version[[:space:]]*=/ { q=index($$0, "\""); if (q>0) { rem=substr($$0, q+1); r=index(rem, "\""); if (r>0) { print substr(rem, 1, r-1); exit } } }' Cargo.toml
  CARGO_NAME_CMD := awk 'BEGIN{p=0} /^\[package\]/{p=1;next} /^\[/{p=0} p && $$0 ~ /^[[:space:]]*name[[:space:]]*=/ { q=index($$0, "\""); if (q>0) { rem=substr($$0, q+1); r=index(rem, "\""); if (r>0) { print substr(rem, 1, r-1); exit } } }' Cargo.toml
endif
VERSION ?= $(shell $(CARGO_VERSION_CMD))
ifeq ($(strip $(VERSION)),)
VERSION := $(shell git describe --tags --always 2>/dev/null || echo 0.0.0)
endif


# macOS app packaging variables
APP_NAME ?= $(BIN_NAME)
APP_BUNDLE_ID ?= ch.migros.aifo-coder
DMG_NAME ?= $(APP_NAME)-$(VERSION)
APP_ICON ?=
SIGN_IDENTITY ?= Migros AI Foundation Code Signer
NOTARY_PROFILE ?=
DMG_BG ?= images/aifo-sticker-1024x1024-web.jpg

# Install locations (override as needed)
PREFIX ?= /usr/local
DESTDIR ?=
BIN_DIR ?= $(DESTDIR)$(PREFIX)/bin
MAN_DIR ?= $(DESTDIR)$(PREFIX)/share/man
MAN1_DIR ?= $(MAN_DIR)/man1
DOC_DIR ?= $(DESTDIR)$(PREFIX)/share/doc/$(BIN_NAME)
EXAMPLES_DIR ?= $(DOC_DIR)/examples

# Build release binaries and package archives for macOS and Linux (Ubuntu/Arch)
# Requires: cargo; install non-native targets via rustup and any required linkers
.PHONY: release-for-target release-for-mac release-for-linux release
release-for-target:
	@set -e; \
	BIN="$(BIN_NAME)"; \
	VERSION="$(VERSION)"; \
	DIST="$(DIST_DIR)"; \
	mkdir -p "$$DIST"; \
	echo "Building release version: $$VERSION"; \
	rm -f Cargo.lock || true; \
	PATH="$$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$$PATH"; \
	CHANNEL="$${AIFO_CODER_RUST_CHANNEL:-stable}"; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	DOCKER_PLATFORM_ARGS=""; \
	case "$$OS" in \
	  MINGW*|MSYS*|CYGWIN*|Windows_NT) DOCKER_PLATFORM_ARGS="" ;; \
	  *) case "$$ARCH" in \
	       x86_64|amd64) DOCKER_PLATFORM_ARGS="--platform linux/amd64" ;; \
	       aarch64|arm64) DOCKER_PLATFORM_ARGS="--platform linux/arm64" ;; \
	       *) DOCKER_PLATFORM_ARGS="" ;; \
	     esac ;; \
	esac; \
	if [ -n "$$RELEASE_TARGETS" ]; then \
	  TARGETS="$$RELEASE_TARGETS"; \
	  echo "Using RELEASE_TARGETS from environment: $$TARGETS"; \
	else \
	  RUSTC_HOST="$$(rustc -vV 2>/dev/null | awk '/^host:/{print $$2}')"; \
	  if [ -n "$$RUSTC_HOST" ]; then \
	    TARGETS="$$RUSTC_HOST"; \
	  else \
	    UNAME_S="$$(uname -s 2>/dev/null || echo unknown)"; \
	    UNAME_M="$$(uname -m 2>/dev/null || echo unknown)"; \
	    case "$$UNAME_S" in \
	      Linux) \
	        case "$$UNAME_M" in \
	          x86_64) TARGETS="x86_64-unknown-linux-gnu" ;; \
	          aarch64|arm64) TARGETS="aarch64-unknown-linux-gnu" ;; \
	          armv7l) TARGETS="armv7-unknown-linux-gnueabihf" ;; \
	          *) TARGETS="" ;; \
	        esac ;; \
	      Darwin) \
	        case "$$UNAME_M" in \
	          x86_64) TARGETS="x86_64-apple-darwin" ;; \
	          arm64) TARGETS="aarch64-apple-darwin" ;; \
	          *) TARGETS="" ;; \
	        esac ;; \
	      *) TARGETS="" ;; \
	    esac; \
	  fi; \
	  case "$$TARGETS" in *apple-darwin) echo "macOS detected: not auto-adding Linux cross targets. Set RELEASE_TARGETS to build additional targets." ;; esac; \
	  echo "No RELEASE_TARGETS specified; defaulting to: $$TARGETS"; \
	fi; \
	for t in $$TARGETS; do \
	  case "$$t" in \
	    *apple-darwin) \
	      if [ "$$(uname -s 2>/dev/null)" = "Darwin" ]; then \
	        echo "Building macOS target $$t with host Rust toolchain ..."; \
	        if command -v rustup >/dev/null 2>&1; then rustup run "$$CHANNEL" cargo build --release --target "$$t"; else cargo build --release --target "$$t"; fi; \
	      else \
	        echo "Skipping macOS target $$t on non-Darwin host"; \
	      fi ;; \
	    *) \
	      echo "Building target $$t inside $(RUST_BUILDER_IMAGE) ..."; \
	      MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	        -v "$$PWD:/workspace" \
	        -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	        -v "$$HOME/.cargo/git:/root/.cargo/git" \
	        -v "$$PWD/target:/workspace/target" \
	        $(RUST_BUILDER_IMAGE) cargo build --release --target "$$t" \
	      || echo "Warning: build failed for $$t";; \
	  esac; \
	done; \
	[ -n "$$BIN" ] || BIN="$$( $(CARGO_NAME_CMD) )"; \
	D="$${DIST:-$(DIST_DIR)}"; \
	V="$${VERSION:-$(VERSION)}"; \
	mkdir -p "$$D"; \
	echo "Packaging artifacts into $$D (binary: $$BIN, version: $$V) ..."; \
	PACKED=0; \
	for t in $$TARGETS; do \
	  case "$$t" in \
	    *apple-darwin) OS=macos ;; \
	    *linux-gnu) OS=linux ;; \
	    *) OS=unknown ;; \
	  esac; \
	  ARCH="$${t%%-*}"; \
	  BIN_US="$$(printf '%s' "$$BIN" | tr '-' '_')"; \
	  BINPATH="target/$$t/release/$$BIN"; \
	  [ -f "$$BINPATH" ] || BINPATH="target/$$t/release/$$BIN_US"; \
	  if [ ! -f "$$BINPATH" ]; then \
	    echo "Skipping $$t (binary not found at $$BINPATH)"; \
	    continue; \
	  fi; \
	  PKG="$$BIN-$$V-$$OS-$$ARCH"; \
	  STAGE="$$D/$$PKG"; \
	  rm -rf "$$STAGE"; install -d -m 0755 "$$STAGE"; \
	  install -m 0755 "$$BINPATH" "$$STAGE/$$BIN"; \
	  [ -f README.md ] && install -m 0644 README.md "$$STAGE/"; \
	  [ -d docs ] && cp -a docs "$$STAGE/"; \
	  [ -d examples ] && cp -a examples "$$STAGE/"; \
	  chmod -R u=rwX,go=rX "$$STAGE" || true; \
	  tar -C "$$D" -czf "$$D/$$PKG.tar.gz" "$$PKG"; \
	  chmod 0644 "$$D/$$PKG.tar.gz" || true; \
	  echo "Wrote $$D/$$PKG.tar.gz"; \
	  rm -rf "$$STAGE"; \
	  PACKED=1; \
	done; \
	if [ "$$PACKED" -eq 0 ]; then \
	  echo "TARGETS were empty or mismatched; scanning target/*/release ..."; \
	  BIN_US="$$(printf '%s' "$$BIN" | tr '-' '_')"; \
	  for dir in target/*/release; do \
	    [ -d "$$dir" ] || continue; \
	    t="$$(basename "$$(dirname "$$dir")")"; \
	    case "$$t" in \
	      *apple-darwin) OS=macos ;; \
	      *linux-gnu) OS=linux ;; \
	      *) OS=unknown ;; \
	    esac; \
	    ARCH="$${t%%-*}"; \
	    for f in "$$dir/$$BIN" "$$dir/$$BIN_US"; do \
	      [ -f "$$f" ] || continue; \
	      PKG="$$BIN-$$V-$$OS-$$ARCH"; \
	      STAGE="$$D/$$PKG"; \
	      rm -rf "$$STAGE"; install -d -m 0755 "$$STAGE"; \
	      install -m 0755 "$$f" "$$STAGE/$$BIN"; \
	      [ -f README.md ] && install -m 0644 README.md "$$STAGE/"; \
	      [ -d docs ] && cp -a docs "$$STAGE/"; \
	      [ -d examples ] && cp -a examples "$$STAGE/"; \
	      chmod -R u=rwX,go=rX "$$STAGE" || true; \
	      tar -C "$$D" -czf "$$D/$$PKG.tar.gz" "$$PKG"; \
	      chmod 0644 "$$D/$$PKG.tar.gz" || true; \
	      echo "Wrote $$D/$$PKG.tar.gz"; \
	      rm -rf "$$STAGE"; \
	      PACKED=1; \
	    done; \
	  done; \
	fi; \
	if [ "$$PACKED" -eq 0 ]; then \
	  echo "No built binaries found to package. Searched TARGETS and target/*/release."; \
	fi; \
	echo Generate checksums for archives (tar.gz, dmg) > /dev/null; \
	if ls "$$D"/*.tar.gz >/dev/null 2>&1 || ls "$$D"/*.dmg >/dev/null 2>&1; then \
	  OUT="$$D/SHA256SUMS.txt"; : > "$$OUT"; \
	  for f in "$$D"/*.tar.gz "$$D"/*.dmg; do \
	    [ -f "$$f" ] || continue; \
	    if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$$f" >> "$$OUT"; \
	    elif command -v sha256sum >/dev/null 2>&1; then sha256sum "$$f" >> "$$OUT"; \
	    else echo "Warning: no shasum/sha256sum found; skipping checksums." >&2; fi; \
	  done; \
	  chmod 0644 "$$OUT" || true; \
	  echo "Wrote $$OUT"; \
	fi; \
	echo Generate SBOM via cargo-cyclonedx (this tool writes <package>.cdx.{json,xml} into the project root) >/dev/null; \
	if command -v cargo >/dev/null 2>&1 && cargo cyclonedx -h >/dev/null 2>&1; then \
	  PKG="$$( $(CARGO_NAME_CMD) )"; \
	  OUT_JSON="$$D/SBOM.cdx.json"; OUT_XML="$$D/SBOM.cdx.xml"; \
	  rm -f "$$OUT_JSON" "$$OUT_XML"; \
	  if cargo cyclonedx --help 2>&1 | grep -q -- '--format'; then \
	    cargo cyclonedx --format json || true; \
	    if [ -s "$$PKG.cdx.json" ]; then cp "$$PKG.cdx.json" "$$OUT_JSON"; fi; \
	  fi; \
	  if [ ! -s "$$OUT_JSON" ]; then \
	    cargo cyclonedx || true; \
	    if [ -s "$$PKG.cdx.json" ]; then cp "$$PKG.cdx.json" "$$OUT_JSON"; \
	    elif [ -s "$$PKG.cdx.xml" ]; then cp "$$PKG.cdx.xml" "$$OUT_XML"; fi; \
	  fi; \
	  if [ -s "$$OUT_JSON" ]; then chmod 0644 "$$OUT_JSON" || true; echo "Wrote $$OUT_JSON"; \
	  elif [ -s "$$OUT_XML" ]; then chmod 0644 "$$OUT_XML" || true; echo "Wrote $$OUT_XML"; \
	  else echo "Warning: cargo-cyclonedx did not produce output files; no SBOM written" >&2; fi; \
	else \
	  echo "cargo-cyclonedx not installed; skipping SBOM. Install with: cargo install cargo-cyclonedx" >&2; \
	fi

release-for-mac:
	@$(MAKE) RELEASE_TARGETS=aarch64-apple-darwin release-for-target

release-for-linux:
	@$(MAKE) RELEASE_TARGETS=x86_64-unknown-linux-gnu release-for-target

# Build both mac (host) and Linux, and also build launcher and mac app/dmg
ifeq ($(shell uname -s),Darwin)
release: rebuild build-launcher release-app release-dmg-sign release-for-mac release-for-linux
else
release: rebuild build-launcher release-for-linux
endif

.PHONY: install
install: build build-launcher
	@set -e; \
	BIN="$(BIN_NAME)"; \
	BINPATH="target/release/$$BIN"; \
	BIN_US="$$(printf '%s' "$$BIN" | tr '-' '_')"; \
	[ -f "$$BINPATH" ] || BINPATH="target/release/$$BIN_US"; \
	if [ ! -f "$$BINPATH" ]; then \
	  OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	  ARCH="$$(uname -m 2>/dev/null || echo unknown)"; \
	  TGT=""; \
	  case "$$OS" in \
	    MINGW*|MSYS*|CYGWIN*|Windows_NT) TGT="x86_64-pc-windows-gnu" ;; \
	    Darwin) case "$$ARCH" in arm64|aarch64) TGT="aarch64-apple-darwin" ;; x86_64) TGT="x86_64-apple-darwin" ;; esac ;; \
	    *) case "$$ARCH" in x86_64|amd64) TGT="x86_64-unknown-linux-gnu" ;; aarch64|arm64) TGT="aarch64-unknown-linux-gnu" ;; esac ;; \
	  esac; \
	  if [ -n "$$TGT" ]; then \
	    BINPATH="target/$$TGT/release/$$BIN"; \
	    [ -f "$$BINPATH" ] || BINPATH="target/$$TGT/release/$$BIN_US"; \
	  fi; \
	fi; \
	if [ ! -f "$$BINPATH" ]; then echo "Error: binary not found for install. Tried target/release and target/$$TGT/release." >&2; exit 1; fi; \
	SUDO=""; if command -v sudo >/dev/null 2>&1 && [ -z "$(DESTDIR)" ]; then SUDO="sudo"; fi; \
	$$SUDO install -d -m 0755 "$(BIN_DIR)" "$(MAN1_DIR)" "$(DOC_DIR)"; \
	$$SUDO install -m 0755 "$$BINPATH" "$(BIN_DIR)/$$BIN"; \
	if [ -f man/$$BIN.1 ]; then \
	  if command -v gzip >/dev/null 2>&1; then \
	    TMP="$$(mktemp)"; cp "man/$$BIN.1" "$$TMP"; gzip -c "$$TMP" | $$SUDO tee "$(MAN1_DIR)/$$BIN.1.gz" >/dev/null; rm -f "$$TMP"; \
	  else \
	    $$SUDO install -m 0644 "man/$$BIN.1" "$(MAN1_DIR)/$$BIN.1"; \
	  fi; \
	fi; \
	[ -f README.md ] && $$SUDO install -m 0644 README.md "$(DOC_DIR)/" || true; \
	[ -f LICENSE ] && $$SUDO install -m 0644 LICENSE "$(DOC_DIR)/" || true; \
	if [ -d examples ]; then \
	  $$SUDO install -d -m 0755 "$(EXAMPLES_DIR)"; \
	  $$SUDO cp -a examples/. "$(EXAMPLES_DIR)/"; \
	  $$SUDO chmod -R u=rwX,go=rX "$(EXAMPLES_DIR)" || true; \
	fi; \
	echo "Installed $$BIN to $(BIN_DIR), man page to $(MAN1_DIR), docs to $(DOC_DIR)"

.PHONY: checksums
checksums:
	@set -e; \
	D="$(DIST_DIR)"; \
	mkdir -p "$$D"; \
	OUT="$$D/SHA256SUMS.txt"; : > "$$OUT"; \
	FOUND=0; \
	for f in "$$D"/*.tar.gz "$$D"/*.dmg; do \
	  [ -f "$$f" ] || continue; FOUND=1; \
	  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$$f" >> "$$OUT"; \
	  elif command -v sha256sum >/dev/null 2>&1; then sha256sum "$$f" >> "$$OUT"; \
	  else echo "Warning: no shasum/sha256sum found; skipping $$f" >&2; fi; \
	done; \
	if [ "$$FOUND" -eq 1 ]; then chmod 0644 "$$OUT" || true; echo "Wrote $$OUT"; else echo "No artifacts found in $$D"; fi

.PHONY: sbom
sbom:
	@set -e; \
	D="$(DIST_DIR)"; \
	mkdir -p "$$D"; \
	if command -v cargo >/dev/null 2>&1 && cargo cyclonedx -h >/dev/null 2>&1; then \
	  PKG="$$( $(CARGO_NAME_CMD) )"; \
	  OUT_JSON="$$D/SBOM.cdx.json"; OUT_XML="$$D/SBOM.cdx.xml"; \
	  rm -f "$$OUT_JSON" "$$OUT_XML"; \
	  if cargo cyclonedx --help 2>&1 | grep -q -- '--format'; then \
	    cargo cyclonedx --format json; \
	    if [ -s "$$PKG.cdx.json" ]; then cp "$$PKG.cdx.json" "$$OUT_JSON"; fi; \
	  fi; \
	  if [ ! -s "$$OUT_JSON" ]; then \
	    cargo cyclonedx; \
	    if [ -s "$$PKG.cdx.json" ]; then cp "$$PKG.cdx.json" "$$OUT_JSON"; \
	    elif [ -s "$$PKG.cdx.xml" ]; then cp "$$PKG.cdx.xml" "$$OUT_XML"; fi; \
	  fi; \
	  if [ -s "$$OUT_JSON" ]; then chmod 0644 "$$OUT_JSON" || true; echo "Wrote $$OUT_JSON"; \
	  elif [ -s "$$OUT_XML" ]; then chmod 0644 "$$OUT_XML" || true; echo "Wrote $$OUT_XML"; \
	  else echo "SBOM generation failed: cargo-cyclonedx produced no files" >&2; exit 1; fi; \
	else \
	  echo "cargo-cyclonedx not installed; install with: cargo install cargo-cyclonedx" >&2; \
	  exit 1; \
	fi

.PHONY: loc
loc:
	@set -e; \
	printf "\nCounting lines of source in repository...\n\n"; \
	count() { \
	  pat="$$1"; \
	  eval "find . \\( -path './.git' -o -path './target' -o -path './dist' -o -path './build' -o -path './node_modules' \\) -prune -o -type f \\( $${pat} \\) -print0" \
	    | xargs -0 wc -l 2>/dev/null | awk 'END{print ($$1+0)}'; \
	}; \
	rust_src=$$(count "-path './src/*' -a -name '*.rs' -o -name 'build.rs'"); \
	rust_tests=$$(count "-path './tests/*' -a -name '*.rs'"); \
	shell=$$(count "-name '*.sh' -o -name '*.bash' -o -name '*.zsh'"); \
	makef=$$(count "-name 'Makefile' -o -name '*.mk'"); \
	docker=$$(count "-name 'Dockerfile' -o -name '*.dockerfile'"); \
	yaml=$$(count "-name '*.yml' -o -name '*.yaml'"); \
	toml=$$(count "-name '*.toml'"); \
	json=$$(count "-name '*.json'"); \
	md=$$(count "-name '*.md'"); \
	other=$$(count "-name '*.conf'"); \
	total=$$((rust_src+rust_tests+shell+makef+docker+yaml+toml+json+md+other)); \
	printf "Lines of code (excluding .git, target, dist, build, node_modules):\n\n"; \
	printf "  Rust source:     %8d  (src/, build.rs)\n" "$$rust_src"; \
	printf "  Rust tests:      %8d  (tests/)\n" "$$rust_tests"; \
	printf "  Shell scripts:   %8d\n" "$$shell"; \
	printf "  Makefiles:       %8d\n" "$$makef"; \
	printf "  Dockerfiles:     %8d\n" "$$docker"; \
	printf "  YAML:            %8d\n" "$$yaml"; \
	printf "  TOML:            %8d\n" "$$toml"; \
	printf "  JSON:            %8d\n" "$$json"; \
	printf "  Markdown:        %8d\n" "$$md"; \
	printf "  Other (.conf):   %8d\n" "$$other"; \
	printf "  -------------------------\n"; \
	printf "  Total:           %8d\n\n" "$$total"

.PHONY: lint-docker
lint-docker:
	@set -e; \
	if command -v hadolint >/dev/null 2>&1; then \
	  echo; \
	  echo "Running hadolint on Dockerfile(s) ..."; \
	  hadolint Dockerfile || true; \
	  if [ -f toolchains/rust/Dockerfile ]; then hadolint toolchains/rust/Dockerfile || true; fi; \
	  if [ -f toolchains/cpp/Dockerfile ]; then hadolint toolchains/cpp/Dockerfile || true; fi; \
	  if [ -f toolchains/node/Dockerfile ]; then hadolint toolchains/node/Dockerfile || true; fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo; \
	  echo "hadolint not found; using hadolint/hadolint container ..."; \
	  docker run --rm -i hadolint/hadolint < Dockerfile || true; \
	  if [ -f toolchains/rust/Dockerfile ]; then docker run --rm -i hadolint/hadolint < toolchains/rust/Dockerfile || true; fi; \
	  if [ -f toolchains/cpp/Dockerfile ]; then docker run --rm -i hadolint/hadolint < toolchains/cpp/Dockerfile || true; fi; \
	  if [ -f toolchains/node/Dockerfile ]; then docker run --rm -i hadolint/hadolint < toolchains/node/Dockerfile || true; fi; \
	else \
	  echo "Warning: hadolint not installed and docker unavailable; skipping Dockerfile lint."; \
	  echo "Install hadolint locally or rely on CI's lint-dockerfiles job."; \
	fi

.PHONY: lint-tests-naming
lint-tests-naming:
	@echo
	@echo "Running test naming lint ..."
	@sh scripts/lint-test-naming.sh --strict

.PHONY: release-app release-dmg release-dmg-sign
ifeq ($(shell uname -s),Darwin)

release-app:
	@( \
	BIN="$(BIN_NAME)"; \
	VERSION="$(VERSION)"; \
	DIST="$(DIST_DIR)"; \
	APP="$(APP_NAME)"; \
	BUNDLE_ID="$(APP_BUNDLE_ID)"; \
	APP_ICON="$(APP_ICON)"; \
	mkdir -p "$$DIST"; \
	arch="$$(uname -m)"; \
	case "$$arch" in \
	  arm64|aarch64) TGT="aarch64-apple-darwin" ;; \
	  x86_64) TGT="x86_64-apple-darwin" ;; \
	  *) echo "Unsupported macOS architecture: $$arch" >&2; exit 1 ;; \
	esac; \
	echo "Building $$BIN for $$TGT ..."; \
	if command -v rustup >/dev/null 2>&1; then \
	  rustup target add "$$TGT" >/dev/null 2>&1 || true; \
	  rustup run stable cargo build --release --target "$$TGT"; \
	else \
	  cargo build --release --target "$$TGT"; \
	fi; \
	BINPATH="target/$$TGT/release/$$BIN"; \
	BIN_US="$$(printf '%s' "$$BIN" | tr '-' '_')"; \
	[ -f "$$BINPATH" ] || BINPATH="target/$$TGT/release/$$BIN_US"; \
	if [ ! -f "$$BINPATH" ]; then \
	  echo "Binary not found at $$BINPATH" >&2; \
	  exit 1; \
	fi; \
	APPROOT="$$DIST/$$APP.app"; \
	CONTENTS="$$APPROOT/Contents"; \
	MACOS="$$CONTENTS/MacOS"; \
	RES="$$CONTENTS/Resources"; \
	rm -rf "$$APPROOT"; \
	install -d -m 0755 "$$MACOS" "$$RES"; \
	install -m 0755 "$$BINPATH" "$$MACOS/$$BIN"; \
	if [ -n "$$APP_ICON" ] && [ -f "$$APP_ICON" ]; then \
	  ICON_DST="$$RES/AppIcon.icns"; \
	  cp "$$APP_ICON" "$$ICON_DST"; \
	fi; \
	printf '%s\n' \
'<?xml version="1.0" encoding="UTF-8"?>' \
'<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
'<plist version="1.0">' \
'<dict>' \
'  <key>CFBundleName</key>' \
"  <string>$$APP</string>" \
'  <key>CFBundleDisplayName</key>' \
"  <string>$$APP</string>" \
'  <key>CFBundleIdentifier</key>' \
"  <string>$$BUNDLE_ID</string>" \
'  <key>CFBundleVersion</key>' \
"  <string>$$VERSION</string>" \
'  <key>CFBundleShortVersionString</key>' \
"  <string>$$VERSION</string>" \
'  <key>CFBundleExecutable</key>' \
"  <string>$$BIN</string>" \
'  <key>CFBundleIconFile</key>' \
'  <string>AppIcon</string>' \
'  <key>LSMinimumSystemVersion</key>' \
'  <string>11.0</string>' \
'</dict>' \
'</plist>' \
> "$$CONTENTS/Info.plist"; \
	echo "Info.plist written. Preview:"; \
	/usr/libexec/PlistBuddy -c 'Print' "$$CONTENTS/Info.plist" 2>/dev/null || cat "$$CONTENTS/Info.plist"; \
	echo "App bundle layout:"; \
	( cd "$$APPROOT" && find . -maxdepth 5 -print | sed -e 's#^\./##' ); \
	echo "Built $$APPROOT (unsigned). Use 'make release-dmg-sign' to sign and create a signed DMG."; \
	)

release-dmg: release-app
	@set -e; \
	BIN="$(BIN_NAME)"; \
	VERSION="$(VERSION)"; \
	DIST="$(DIST_DIR)"; \
	APP="$(APP_NAME)"; \
	DMG="$(DMG_NAME)"; \
	APPROOT="$$DIST/$$APP.app"; \
	if [ ! -d "$$APPROOT" ]; then echo "App bundle not found at $$APPROOT; run 'make release-app' first." >&2; exit 1; fi; \
	DMG_PATH="$$DIST/$$DMG.dmg"; \
	BG_SRC="$(DMG_BG)"; \
	if ! command -v hdiutil >/dev/null 2>&1; then echo "hdiutil not found; cannot build DMG." >&2; exit 1; fi; \
	echo "Creating $$DMG_PATH (with background and layout) ..."; \
	STAGE="$$DIST/.dmg-root"; rm -rf "$$STAGE"; mkdir -p "$$STAGE/.background"; \
	ln -s /Applications "$$STAGE/Applications"; \
	cp -a "$$APPROOT" "$$STAGE/"; \
	if [ -f "$$BG_SRC" ]; then \
	  BG_NAME="$$(basename "$$BG_SRC")"; \
	  cp "$$BG_SRC" "$$STAGE/.background/$$BG_NAME"; \
	else \
	  echo "Warning: DMG background not found at $$BG_SRC; proceeding without background." >&2; \
	  BG_NAME=""; \
	fi; \
	echo Create temporary read-write DMG to customize Finder view >/dev/null; \
	TMP_DMG="$$DIST/.tmp-$$DMG.dmg"; \
	MNT="$$DIST/.mnt-$$APP"; \
	rm -f "$$TMP_DMG"; \
	hdiutil create -ov -fs HFS+J -srcfolder "$$STAGE" -volname "$$APP" -format UDRW "$$TMP_DMG"; \
	rm -rf "$$STAGE"; \
	mkdir -p "$$MNT"; \
	hdiutil attach -readwrite -noverify -noautoopen -mountpoint "$$MNT" "$$TMP_DMG" >/dev/null; \
	echo Configure Finder window via AppleScript \(best-effort\) >/dev/null; \
	if command -v osascript >/dev/null 2>&1; then \
	  BG_LINE=""; \
	  osascript <<EOF || true \
tell application "Finder" \
  tell disk "$$APP" \
    open \
    set current view of container window to icon view \
    set toolbar visible of container window to false \
    set statusbar visible of container window to false \
    set bounds of container window to {100, 100, 680, 480} \
    set opts to the icon view options of container window \
    set arrangement of opts to not arranged \
    set icon size of opts to 96 \
    if "$$BG_NAME" is not "" then \
      try \
        set background picture of opts to POSIX file "$$MNT/.background/$$BG_NAME" \
      end try \
    end if \
    delay 1 \
    set position of item "$$APP.app" of container window to {120, 260} \
    set position of item "Applications" of container window to {360, 260} \
    close \
    open \
    update without registering applications \
    delay 1 \
  end tell \
end tell \
EOF \
	else \
	  echo "osascript not available; skipping Finder customization." >&2; \
	fi; \
	echo Detach and convert to compressed DMG >/dev/null; \
	hdiutil detach "$$MNT" -quiet || hdiutil detach "$$MNT" -force -quiet || true; \
	rm -rf "$$MNT"; \
	hdiutil convert "$$TMP_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$$DMG_PATH" >/dev/null; \
	rm -f "$$TMP_DMG"; \
	echo "Wrote $$DMG_PATH (unsigned)";

# Sign the .app and .dmg (and optionally notarize). This target signs the app first,
# then rebuilds the DMG so it contains the signed app, then signs the DMG.
release-dmg-sign: release-app
	@/bin/sh -ec '\
	APP="$(APP_NAME)"; \
	BIN="$(BIN_NAME)"; \
	DIST="$(DIST_DIR)"; \
	APPROOT="$$DIST/$$APP.app"; \
	DMG_NAME="$(DMG_NAME)"; \
	DMG_PATH="$$DIST/$$DMG_NAME.dmg"; \
	SIGN_ID_NAME="$(SIGN_IDENTITY)"; \
	echo "Preparing to sign macOS app and DMG..."; \
	if [ ! -d "$$APPROOT" ]; then echo "Error: app bundle not found at $$APPROOT. Run '"'"'make release-app'"'"' first." >&2; exit 1; fi; \
	command -v security >/dev/null 2>&1 || { echo "Error: security tool not found (macOS required)"; exit 1; }; \
	command -v codesign >/dev/null 2>&1 || { echo "Error: codesign tool not found (Xcode Command Line Tools)"; exit 1; }; \
	echo "Using signing identity name: $$SIGN_ID_NAME"; \
	echo "Keychains (user):"; security list-keychains -d user || true; \
	echo "Default keychain (user):"; security default-keychain -d user || true; \
	echo "Available code signing identities (may be empty for self-signed certs):"; \
	security find-identity -p codesigning -v || true; \
	echo "Available identities (basic listing, may include self-signed):"; \
	security find-identity -p basic -v || true; \
	echo "Searching for certificate by common name (including self-signed/untrusted):"; \
	security find-certificate -a -c "$$SIGN_ID_NAME" -Z 2>/dev/null | sed -n "1,12p" || true; \
	CERT_MATCH_COUNT="$$(security find-certificate -a -c "$$SIGN_ID_NAME" -Z 2>/dev/null | grep -c "^SHA-1 hash:" || true)"; \
	if [ "$$CERT_MATCH_COUNT" -eq 0 ]; then \
	  echo "Error: No certificate with common name $$SIGN_ID_NAME found in your keychains." >&2; \
	  echo "Hint: Ensure the certificate AND its private key are in the login keychain and unlocked, or override SIGN_IDENTITY." >&2; \
	  exit 1; \
	fi; \
	KEYCHAIN="$$(security default-keychain -d user | sed -e "s/[ \"]//g")"; \
	echo "Default keychain (path): $${KEYCHAIN:-unknown}"; \
	SIG_SHA1="$$(security find-certificate -a -c "$$SIGN_ID_NAME" -Z 2>/dev/null | awk '\''/^SHA-1 hash:/{print $$3; exit}'\'')"; \
	if [ -n "$$SIG_SHA1" ]; then \
	  echo "Found certificate SHA-1: $$SIG_SHA1"; \
	else \
	  echo "Warning: Could not extract SHA-1 hash from certificate lookup."; \
	fi; \
	APPLE_DEV=0; \
	if command -v openssl >/dev/null 2>&1; then \
	  SUBJ="$$(security find-certificate -a -c "$$SIGN_ID_NAME" -p 2>/dev/null | openssl x509 -noout -subject 2>/dev/null | head -n1)"; \
	  echo "Certificate subject: $${SUBJ:-unknown}"; \
	  case "$$SUBJ" in *"Developer ID Application"*|*"Apple Distribution"*|*"Apple Development"*) APPLE_DEV=1 ;; esac; \
	fi; \
	if [ "$$APPLE_DEV" -eq 1 ]; then \
	  SIGN_FLAGS="--force --verbose=4 --options runtime --timestamp"; \
	  echo "Using hardened runtime signing flags (Apple Developer identity detected)."; \
	else \
	  SIGN_FLAGS="--force --verbose=4"; \
	  echo "Using basic signing flags (self-signed or non-Apple certificate)."; \
	fi; \
	BIN_EXEC="$$APPROOT/Contents/MacOS/$$BIN"; \
	if [ ! -x "$$BIN_EXEC" ]; then echo "Error: app executable not found at $$BIN_EXEC" >&2; exit 1; fi; \
	echo "Clearing extended attributes on app bundle (xattr -cr) ..."; \
	if command -v xattr >/dev/null 2>&1; then xattr -cr "$$APPROOT" || true; fi; \
	echo "Signing inner executable: $$BIN_EXEC"; \
	if codesign $$SIGN_FLAGS --keychain "$$KEYCHAIN" -s "$$SIGN_ID_NAME" "$$BIN_EXEC" >/dev/null 2>&1; then \
	  echo "Signed inner executable with identity name via default keychain."; \
	elif [ -n "$$SIG_SHA1" ] && codesign $$SIGN_FLAGS --keychain "$$KEYCHAIN" -s "$$SIG_SHA1" "$$BIN_EXEC" >/dev/null 2>&1; then \
	  echo "Signed inner executable with certificate SHA-1 via default keychain."; \
	elif codesign $$SIGN_FLAGS -s "$$SIGN_ID_NAME" "$$BIN_EXEC" >/dev/null 2>&1; then \
	  echo "Signed inner executable with identity name (no explicit keychain)."; \
	else \
	  echo "Warning: could not use signing identity '$$SIGN_ID_NAME' (or SHA-1) for inner executable; falling back to ad-hoc." >&2; \
	  if codesign $$SIGN_FLAGS -s - "$$BIN_EXEC"; then \
	    echo "Ad-hoc signed inner executable (no identity)."; \
	  else \
	    echo "codesign inner executable failed" >&2; exit 1; \
	  fi; \
	fi; \
	echo "Signing app bundle: $$APPROOT"; \
	echo "Use --deep to ensure nested components are signed if present" >/dev/null; \
	if codesign $$SIGN_FLAGS --deep --keychain "$$KEYCHAIN" -s "$$SIGN_ID_NAME" "$$APPROOT" >/dev/null 2>&1; then \
	  echo "Signed app bundle with identity name via default keychain."; \
	elif [ -n "$$SIG_SHA1" ] && codesign $$SIGN_FLAGS --deep --keychain "$$KEYCHAIN" -s "$$SIG_SHA1" "$$APPROOT" >/dev/null 2>&1; then \
	  echo "Signed app bundle with certificate SHA-1 via default keychain."; \
	elif codesign $$SIGN_FLAGS --deep -s "$$SIGN_ID_NAME" "$$APPROOT" >/dev/null 2>&1; then \
	  echo "Signed app bundle with identity name (no explicit keychain)."; \
	else \
	  echo "Warning: could not use signing identity '$$SIGN_ID_NAME' (or SHA-1) for app bundle; falling back to ad-hoc." >&2; \
	  if codesign $$SIGN_FLAGS --deep -s - "$$APPROOT"; then \
	    echo "Ad-hoc signed app bundle (no identity)."; \
	  else \
	    echo "codesign app bundle failed" >&2; exit 1; \
	  fi; \
	fi; \
	echo "Verifying app signature (deep/strict) ..."; \
	if ! codesign --verify --deep --strict --verbose=4 "$$APPROOT"; then \
	  echo "codesign verification failed for app" >&2; exit 1; \
	fi; \
	echo "Building DMG from signed app ..."; \
	$(MAKE) release-dmg; \
	if [ ! -f "$$DMG_PATH" ]; then echo "Error: DMG not found at $$DMG_PATH" >&2; exit 1; fi; \
	echo "Clearing extended attributes on DMG (xattr -cr) ..."; \
	if command -v xattr >/dev/null 2>&1; then xattr -cr "$$DMG_PATH" || true; fi; \
	echo "Signing DMG at $$DMG_PATH ..."; \
	if codesign --force --verbose=4 --keychain "$$KEYCHAIN" -s "$$SIGN_ID_NAME" "$$DMG_PATH" >/dev/null 2>&1; then \
	  echo "Signed DMG with identity name via default keychain."; \
	elif [ -n "$$SIG_SHA1" ] && codesign --force --verbose=4 --keychain "$$KEYCHAIN" -s "$$SIG_SHA1" "$$DMG_PATH" >/dev/null 2>&1; then \
	  echo "Signed DMG with certificate SHA-1 via default keychain."; \
	elif codesign --force --verbose=4 -s "$$SIGN_ID_NAME" "$$DMG_PATH" >/dev/null 2>&1; then \
	  echo "Signed DMG with identity name (no explicit keychain)."; \
	else \
	  echo "Warning: could not use signing identity '$$SIGN_ID_NAME' (or SHA-1) for DMG; falling back to ad-hoc." >&2; \
	  if codesign --force --verbose=4 -s - "$$DMG_PATH"; then \
	    echo "Ad-hoc signed DMG (no identity)."; \
	  else \
	    echo "codesign DMG failed" >&2; exit 1; \
	  fi; \
	fi; \
	NOTARY="$(NOTARY_PROFILE)"; \
	DMG_ADHOC=0; APP_ADHOC=0; \
	DSIG="$$(codesign -dv --verbose=4 "$$DMG_PATH" 2>&1 || true)"; \
	ASIG="$$(codesign -dv --verbose=4 "$$APPROOT" 2>&1 || true)"; \
	echo "$$DSIG" | grep -q "^Authority=" || DMG_ADHOC=1; \
	echo "$$ASIG" | grep -q "^Authority=" || APP_ADHOC=1; \
	if [ "$$DMG_ADHOC" -eq 1 ] || [ "$$APP_ADHOC" -eq 1 ]; then \
	  echo "Skipping notarization: app or DMG were ad-hoc signed (no Apple identity)."; \
	elif [ "$$APPLE_DEV" -eq 1 ] && [ -n "$$NOTARY" ] && command -v xcrun >/dev/null 2>&1 && xcrun notarytool --help >/dev/null 2>&1; then \
	  echo "Submitting $$DMG_PATH for notarization with profile $$NOTARY ..."; \
	  if ! xcrun notarytool submit "$$DMG_PATH" --keychain-profile "$$NOTARY" --wait; then \
	    echo "Notarization failed" >&2; exit 1; \
	  fi; \
	  echo "Stapling notarization ticket to DMG and app ..."; \
	  xcrun stapler staple "$$DMG_PATH" || true; \
	  xcrun stapler staple "$$APPROOT" || true; \
	else \
	  echo "Skipping notarization (NOTARY_PROFILE unset, notarytool unavailable, or non-Apple identity)."; \
	fi; \
	echo "Signing steps completed: $$APPROOT and $$DMG_PATH"; \
	'

else

release-app:
	@echo "release-app is only supported on macOS (Darwin) hosts." >&2; exit 1

release-dmg:
	@echo "release-dmg is only supported on macOS (Darwin) hosts." >&2; exit 1

release-dmg-sign:
	@echo "release-dmg-sign is only supported on macOS (Darwin) hosts." >&2; exit 1

endif

.PHONY: cov-results
cov-results:
	open build/coverage/html/index.html
