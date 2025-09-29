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
#     - Language toolchain sidecars (rust, node/ts, python, c/cpp, go) via secure proxy.
#     - Optional unix:// proxy on Linux; host-gateway bridging when needed.
#     - Minimal mounts: project workspace, config files, optional GnuPG keyrings.
# ──────────────────────────────────────────────────────────────────────────────────────────────
#  📜 Written 2025 by Amir Guindehi <amir.guindehi@mgb.ch>, Head of Migros AI Foundation at MGB
# ──────────────────────────────────────────────────────────────────────────────────────────────
#

# Build one image per agent with shared base layers for maximal cache reuse.
IMAGE_PREFIX ?= aifo-coder
TAG ?= latest
# Set to 1 to keep apt/procps in final images (default drops them in final stages)
KEEP_APT ?= 0

# BuildKit/Buildx configuration
USE_BUILDX ?= 1
PLATFORMS ?=
PUSH ?= 0
CACHE_DIR ?= .buildx-cache

# Nextest niceness
NICENESS_CARGO_NEXTEST =? -1

# Nextest arguments
ARGS_NEXTEST ?= --no-fail-fast --status-level=fail --hide-progress-bar --cargo-quiet

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
	@echo "  CACHE_DIR ................... Local buildx cache directory for faster rebuilds (.buildx-cache)"
	@echo "  ARGS ........................ Extra args passed to tests when running 'make test' (e.g., -- --nocapture)"
	@echo "  CLIPPY ...................... Set to 1 to run 'make lint' before 'make test' (default: off)"
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
	@echo "  build-coder ................. Build both slim and fat images (all agents)"
	@echo "  build-fat ................... Build all fat images (codex, crush, aider)"
	@echo "  build-slim .................. Build all slim images (codex-slim, crush-slim, aider-slim)"
	@echo ""
	@echo "  build-codex ................. Build only the Codex image ($${IMAGE_PREFIX}-codex:$${TAG})"
	@echo "  build-crush ................. Build only the Crush image ($${IMAGE_PREFIX}-crush:$${TAG})"
	@echo "  build-aider ................. Build only the Aider image ($${IMAGE_PREFIX}-aider:$${TAG})"
	@echo "  build-codex-slim ............ Build only the Codex slim image ($${IMAGE_PREFIX}-codex-slim:$${TAG})"
	@echo "  build-crush-slim ............ Build only the Crush slim image ($${IMAGE_PREFIX}-crush-slim:$${TAG})"
	@echo "  build-aider-slim ............ Build only the Aider slim image ($${IMAGE_PREFIX}-aider-slim:$${TAG})"
	@echo ""
	@echo "  build-toolchain ............. Build all toolchain sidecar images (rust/node/cpp)"
	@echo "  build-toolchain-rust ........ Build the Rust toolchain sidecar image (aifo-rust-toolchain:latest)"
	@echo "  build-toolchain-node ........ Build the Node toolchain sidecar image (aifo-node-toolchain:latest)"
	@echo "  build-toolchain-cpp ......... Build the C-CPP toolchain sidecar image (aifo-cpp-toolchain:latest)"
	@echo ""
	@echo "  build-rust-builder .......... Build the Rust cross-compile builder image ($${IMAGE_PREFIX}-rust-builder:$${TAG})"
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
	@echo "  rebuild-codex-slim .......... Rebuild only the Codex slim image without cache"
	@echo "  rebuild-crush-slim .......... Rebuild only the Crush slim image without cache"
	@echo "  rebuild-aider-slim .......... Rebuild only the Aider slim image without cache"
	@echo ""
	@echo "  rebuild-toolchain ........... Rebuild all toolchain sidecar images without cache"
	@echo "  rebuild-toolchain-rust ...... Rebuild only the Rust toolchain image without cache"
	@echo "  rebuild-toolchain-node ...... Rebuild only the Node toolchain image without cache"
	@echo "  rebuild-toolchain-cpp ....... Rebuild only the C-CPP toolchain image without cache"
	@echo ""
	@echo "  rebuild-rust-builder ........ Rebuild only the Rust builder image without cache"
	@echo ""
	$(call title,Rebuild existing images by prefix:)
	@echo ""
	@echo "  rebuild-existing ............ Rebuild any existing local images with IMAGE_PREFIX (using cache)"
	@echo "  rebuild-existing-nocache .... Same, but without cache"
	@echo ""
	$(call title,Publish images:)
	@echo ""
	@echo "  publish-toolchain-rust ...... Buildx multi-arch and push Rust toolchain (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo "  publish-toolchain-node ...... Buildx multi-arch and push Node toolchain (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo "  publish-toolchain-cpp ....... Buildx multi-arch and push C-CPP toolchain (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1)"
	@echo "  publish-codex ............... Buildx multi-arch and push Codex (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-codex-slim .......... Buildx multi-arch and push Codex (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-crush ............... Buildx multi-arch and push Crush (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-crush-slim .......... Buildx multi-arch and push Crush (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-aider ............... Buildx multi-arch and push Aider (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-aider-slim .......... Buildx multi-arch and push Aider (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-openhands ........... Buildx multi-arch and push OpenHands (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-openhands-slim ...... Buildx multi-arch and push OpenHands (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-opencode ............ Buildx multi-arch and push OpenCode (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-opencode-slim ....... Buildx multi-arch and push OpenCode (slim; set PLATFORMS=... PUSH=1)"
	@echo "  publish-plandex ............. Buildx multi-arch and push Plandex (full; set PLATFORMS=... PUSH=1)"
	@echo "  publish-plandex-slim ........ Buildx multi-arch and push Plandex (slim; set PLATFORMS=... PUSH=1)"
	@echo ""
	$(call title,Utilities:)
	@echo ""
	@echo "  clean ....................... Remove built and base images (ignores errors if not present)"
	@echo "  toolchain-cache-clear ....... Purge all toolchain cache Docker volumes (rust/node/npm/pip/ccache/go)"
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
	$(call title,Test targets:)
	@echo ""
	@echo "  check ....................... Run 'lint' then 'test' (composite validation target)"
	@echo ""
	@echo "  lint ........................ Run cargo fmt -- --check and cargo clippy (workspace, all targets/features; -D warnings)"
	@echo "  lint-ultra .................. Run cargo fmt -- --check and cargo clippy (workspace, all targets/features; -D warnings,unsafe_code,clippy::*)"
	@echo ""
	@echo "  cov ......................... Run coverage-html and coverage-lcov (composite target)"
	@echo "  coverage-html ............... Generate HTML coverage via nextest+grcov (rustup/cargo/docker fallback)"
	@echo "  coverage-lcov ............... Generate lcov.info via nextest+grcov (rustup/cargo/docker fallback)"
	@echo ""
	@echo "  test ........................ Run Rust tests with cargo-nextest (installs in container if missing)"
	@echo "  test-cargo .................. Run legacy 'cargo test' (no nextest)"
	@echo "  test-legacy ................. Alias for test-cargo"
	@echo "  test-proxy-smoke ............ Run proxy smoke test (ignored by default)"
	@echo "  test-toolchain-live ......... Run live toolchain tests (ignored by default)"
	@echo "  test-shim-embed ............. Check embedded shim presence in agent image (ignored by default)"
	@echo "  test-proxy-unix ............. Run unix-socket proxy smoke test (ignored by default; Linux-only)"
	@echo "  test-proxy-errors ........... Run proxy error semantics tests (ignored by default)"
	@echo "  test-proxy-tcp .............. Run TCP streaming proxy test (ignored by default)"
	@echo "  test-dev-tool-routing ....... Run dev-tool routing tests (ignored by default)"
	@echo "  test-tsc-resolution ......... Run TypeScript local tsc resolution test (ignored by default)"
	@echo "  test-toolchain-cpp .......... Run c-cpp toolchain dry-run tests"
	@echo "  test-toolchain-rust ......... Run unit/integration rust sidecar tests (exclude ignored/E2E)"
	@echo "  test-toolchain-rust-e2e ..... Run ignored rust sidecar E2E tests (docker required)"
	@echo ""
	$(call title,Test suites:)
	@echo ""
	@echo "  test-acceptance-suite ....... Run acceptance suite (shim/proxy: native HTTP TCP/UDS, wrappers, logs, disconnect, override)"
	@echo "  test-integration-suite ...... Run integration/E2E suite (proxy smoke/unix/errors/tcp, routing, tsc, rust E2E)"
	@echo "  test-e2e-suite .............. Run all ignored-by-default tests (acceptance + integration suites)"
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
	@echo "    aifo-coder aider --toolchain rust -- --watch-files"
	@echo "    aifo-coder codex --toolchain node -- resume"
	@echo "    aifo-coder crush --toolchain ts -- --version"
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
RUST_TOOLCHAIN_TAG ?= latest
NODE_TOOLCHAIN_TAG ?= latest
RUST_BASE_TAG ?= 1-bookworm
NODE_BASE_TAG ?= 22-bookworm-slim
# Optional corporate CA for rust toolchain build; if present, pass as BuildKit secret
MIGROS_CA ?= $(HOME)/.certificates/MigrosRootCA2.crt
COMMA := ,
RUST_CA_SECRET := $(if $(wildcard $(MIGROS_CA)),--secret id=migros_root_ca$(COMMA)src=$(MIGROS_CA),)
CA_SECRET := $(if $(wildcard $(MIGROS_CA)),--secret id=migros_root_ca$(COMMA)src=$(MIGROS_CA),)

.PHONY: build build-coder build-fat build-codex build-crush build-aider build-rust-builder build-launcher
build-fat: build-codex build-crush build-aider build-openhands build-opencode build-plandex

build: build-slim build-fat build-rust-builder build-toolchain build-launcher

build-coder: build-slim build-fat build-rust-builder

build-codex:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) -t "$${REG}$(CODEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) $(CA_SECRET) .; \
	fi

build-crush:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush -t $(CRUSH_IMAGE) -t "$${REG}$(CRUSH_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush -t $(CRUSH_IMAGE) $(CA_SECRET) .; \
	fi

build-aider:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider -t $(AIDER_IMAGE) -t "$${REG}$(AIDER_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider -t $(AIDER_IMAGE) $(CA_SECRET) .; \
	fi

build-openhands:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands -t $(OPENHANDS_IMAGE) -t "$${REG}$(OPENHANDS_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands -t $(OPENHANDS_IMAGE) $(CA_SECRET) .; \
	fi

build-opencode:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode -t $(OPENCODE_IMAGE) -t "$${REG}$(OPENCODE_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode -t $(OPENCODE_IMAGE) $(CA_SECRET) .; \
	fi

build-plandex:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex -t $(PLANDEX_IMAGE) -t "$${REG}$(PLANDEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex -t $(PLANDEX_IMAGE) $(CA_SECRET) .; \
	fi

build-rust-builder:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --target rust-builder -t $(RUST_BUILDER_IMAGE) -t "$${REG}$(RUST_BUILDER_IMAGE)" .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --target rust-builder -t $(RUST_BUILDER_IMAGE) .; \
	fi

.PHONY: build-debug
build-debug:
	@set -e; \
	STAGE="$${STAGE:-aider}"; \
	echo "Debug building stage '$$STAGE' with docker buildx (plain progress) ..."; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	fi; \
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
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
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
	    --target "$$STAGE" \
	    -t "$$OUT" $(CA_SECRET) .; \
	fi

.PHONY: build-toolchain-rust rebuild-toolchain-rust
build-toolchain-rust:
	@set -e; \
	echo "Building aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) ..."; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; using registry prefix for base images."; RP="repository.migros.net/"; \
	  echo "Using base image $${RP}rust:$(RUST_BASE_TAG)"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build rust toolchain image."; \
	    exit 1; \
	  fi; \
	fi; \
	if [ -n "$$RP" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile -t aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) -t "$${RP}aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG)" $(RUST_CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile -t aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) $(RUST_CA_SECRET) .; \
	fi

rebuild-toolchain-rust:
	@set -e; \
	echo "Rebuilding aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) (no cache) ..."; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; using registry prefix for base images."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build rust toolchain image."; \
	    exit 1; \
	  fi; \
	fi; \
	if [ -n "$$RP" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile -t aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) -t "$${RP}aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG)" $(RUST_CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile -t aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) $(RUST_CA_SECRET) .; \
	fi

.PHONY: build-toolchain-node rebuild-toolchain-node
build-toolchain-node:
	@set -e; \
	echo "Building aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) ..."; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; using registry prefix for base images."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build node toolchain image."; \
	    exit 1; \
	  fi; \
	fi; \
	if [ -n "$$RP" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) -t "$${RP}aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG)" $(CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) $(CA_SECRET) .; \
	fi

rebuild-toolchain-node:
	@set -e; \
	echo "Rebuilding aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) (no cache) ..."; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; using registry prefix for base images."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild node toolchain image."; \
	    exit 1; \
	  fi; \
	fi; \
	if [ -n "$$RP" ]; then \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) -t "$${RP}aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG)" $(CA_SECRET) .; \
	else \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) $(CA_SECRET) .; \
	fi

.PHONY: build-toolchain
build-toolchain: build-toolchain-rust build-toolchain-node build-toolchain-cpp

.PHONY: rebuild-toolchain
rebuild-toolchain: rebuild-toolchain-rust rebuild-toolchain-node rebuild-toolchain-cpp

.PHONY: publish-toolchain-rust
publish-toolchain-rust:
	@set -e; \
	echo "Publishing aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) with buildx (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in \
	  */) ;; \
	  "") ;; \
	  *) REG="$$REG/";; \
	esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; using registry prefix for base images."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	fi; \
	if [ "$(PUSH)" = "1" ]; then \
	  if [ -n "$$REG" ]; then \
	    echo "PUSH=1 and REGISTRY specified: pushing to $$REG ..."; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile -t "$${REG}aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG)" $(RUST_CA_SECRET) .; \
	  else \
	    echo "PUSH=1 but no REGISTRY specified; refusing to push to docker.io. Writing multi-arch OCI archive instead."; \
	    mkdir -p dist; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile --output type=oci,dest=dist/aifo-rust-toolchain-$(RUST_TOOLCHAIN_TAG).oci.tar $(RUST_CA_SECRET) .; \
	    echo "Wrote dist/aifo-rust-toolchain-$(RUST_TOOLCHAIN_TAG).oci.tar"; \
	  fi; \
	else \
	  echo "PUSH=0: building locally (single-arch loads into Docker when supported) ..."; \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg RUST_TAG="$(RUST_BASE_TAG)" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/rust/Dockerfile -t aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) $(RUST_CA_SECRET) .; \
	fi

.PHONY: build-toolchain-cpp rebuild-toolchain-cpp
build-toolchain-cpp:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build c-cpp toolchain image."; \
	    exit 1; \
	  fi; \
	fi; \
	if [ -n "$$RP" ]; then \
	  $(DOCKER_BUILD) --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t aifo-cpp-toolchain:latest -t "$${RP}aifo-cpp-toolchain:latest" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t aifo-cpp-toolchain:latest $(CA_SECRET) .; \
	fi

rebuild-toolchain-cpp:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild c-cpp toolchain image."; \
	    exit 1; \
	  fi; \
	fi; \
	if [ -n "$$RP" ]; then \
	  $(DOCKER_BUILD) --no-cache --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t aifo-cpp-toolchain:latest -t "$${RP}aifo-cpp-toolchain:latest" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --no-cache --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t aifo-cpp-toolchain:latest $(CA_SECRET) .; \
	fi

.PHONY: publish-toolchain-cpp
publish-toolchain-cpp:
	@set -e; \
	echo "Publishing aifo-cpp-toolchain:latest with buildx (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1) ..."; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging with registry prefix when pushing."; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; proceeding without prefix unless REGISTRY is set."; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in \
	  */) ;; \
	  "") ;; \
	  *) REG="$$REG/";; \
	esac; \
	if [ "$(PUSH)" = "1" ]; then \
	  if [ -n "$$REG" ]; then \
	    echo "PUSH=1 and REGISTRY specified: pushing to $$REG ..."; \
	    $(DOCKER_BUILD) --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t "$${REG}aifo-cpp-toolchain:latest" $(CA_SECRET) .; \
	  else \
	    echo "PUSH=1 but no REGISTRY specified; refusing to push to docker.io. Writing multi-arch OCI archive instead."; \
	    mkdir -p dist; \
	    $(DOCKER_BUILD) --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile --output type=oci,dest=dist/aifo-cpp-toolchain-latest.oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/aifo-cpp-toolchain-latest.oci.tar"; \
	  fi; \
	else \
	  echo "PUSH=0: building locally (single-arch loads into Docker when supported) ..."; \
	  $(DOCKER_BUILD) --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/cpp/Dockerfile -t aifo-cpp-toolchain:latest $(CA_SECRET) .; \
	fi

.PHONY: publish-toolchain-node
publish-toolchain-node:
	@set -e; \
	echo "Publishing aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) with buildx (set PLATFORMS=linux/amd64,linux/arm64 PUSH=1) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in \
	  */) ;; \
	  "") ;; \
	  *) REG="$$REG/";; \
	esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; using registry prefix for base images."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	fi; \
	if [ "$(PUSH)" = "1" ]; then \
	  if [ -n "$$REG" ]; then \
	    echo "PUSH=1 and REGISTRY specified: pushing to $$REG ..."; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t "$${REG}aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG)" $(CA_SECRET) .; \
	  else \
	    echo "PUSH=1 but no REGISTRY specified; refusing to push to docker.io. Writing multi-arch OCI archive instead."; \
	    mkdir -p dist; \
	    DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile --output type=oci,dest=dist/aifo-node-toolchain-$(NODE_TOOLCHAIN_TAG).oci.tar $(CA_SECRET) .; \
	    echo "Wrote dist/aifo-node-toolchain-$(NODE_TOOLCHAIN_TAG).oci.tar"; \
	  fi; \
	else \
	  echo "PUSH=0: building locally (single-arch loads into Docker when supported) ..."; \
	  DOCKER_BUILDKIT=1 $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" -f toolchains/node/Dockerfile -t aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) $(CA_SECRET) .; \
	fi

# Publish agent images (full and slim). Tags both local and registry-prefixed refs when REGISTRY is set.
.PHONY: publish-openhands publish-openhands-slim publish-opencode publish-opencode-slim publish-plandex publish-plandex-slim

publish-codex:
	@set -e; \
	echo "Publishing $(CODEX_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) -t "$${REG}$(CODEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) $(CA_SECRET) .; \
	fi

publish-codex-slim:
	@set -e; \
	echo "Publishing $(CODEX_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${REG}$(CODEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

publish-crush:
	@set -e; \
	echo "Publishing $(CRUSH_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush -t $(CRUSH_IMAGE) -t "$${REG}$(CRUSH_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush -t $(CRUSH_IMAGE) $(CA_SECRET) .; \
	fi

publish-crush-slim:
	@set -e; \
	echo "Publishing $(CRUSH_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${REG}$(CRUSH_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

publish-aider:
	@set -e; \
	echo "Publishing $(AIDER_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider -t $(AIDER_IMAGE) -t "$${REG}$(AIDER_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider -t $(AIDER_IMAGE) $(CA_SECRET) .; \
	fi

publish-aider-slim:
	@set -e; \
	echo "Publishing $(AIDER_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${REG}$(AIDER_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider-slim -t $(AIDER_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

publish-openhands:
	@set -e; \
	echo "Publishing $(OPENHANDS_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands -t $(OPENHANDS_IMAGE) -t "$${REG}$(OPENHANDS_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands -t $(OPENHANDS_IMAGE) $(CA_SECRET) .; \
	fi

publish-openhands-slim:
	@set -e; \
	echo "Publishing $(OPENHANDS_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) -t "$${REG}$(OPENHANDS_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

publish-opencode:
	@set -e; \
	echo "Publishing $(OPENCODE_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode -t $(OPENCODE_IMAGE) -t "$${REG}$(OPENCODE_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode -t $(OPENCODE_IMAGE) $(CA_SECRET) .; \
	fi

publish-opencode-slim:
	@set -e; \
	echo "Publishing $(OPENCODE_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) -t "$${REG}$(OPENCODE_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

publish-plandex:
	@set -e; \
	echo "Publishing $(PLANDEX_IMAGE) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex -t $(PLANDEX_IMAGE) -t "$${REG}$(PLANDEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex -t $(PLANDEX_IMAGE) $(CA_SECRET) .; \
	fi

publish-plandex-slim:
	@set -e; \
	echo "Publishing $(PLANDEX_IMAGE_SLIM) (set PLATFORMS and PUSH=1 for multi-arch) ..."; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	case "$$REG" in */) ;; "") ;; *) REG="$$REG/";; esac; \
	RP=""; \
	echo "Checking reachability of https://repository.migros.net ..."; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then RP="repository.migros.net/"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) -t "$${REG}$(PLANDEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

.PHONY: build-slim build-codex-slim build-crush-slim build-aider-slim
build-slim: build-codex-slim build-crush-slim build-aider-slim build-openhands-slim build-opencode-slim build-plandex-slim

build-codex-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${REG}$(CODEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-crush-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${REG}$(CRUSH_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-aider-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${REG}$(AIDER_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider-slim -t $(AIDER_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-openhands-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) -t "$${REG}$(OPENHANDS_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-opencode-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) -t "$${REG}$(OPENCODE_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

build-plandex-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot build images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) -t "$${REG}$(PLANDEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target plandex-slim -t $(PLANDEX_IMAGE_SLIM) $(CA_SECRET) .; \
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
	    if command -v rustup >/dev/null 2>&1; then rustup run stable cargo build --release --target "$$TGT"; else cargo build --release --target "$$TGT"; fi; \
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
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) cargo build --release --target "$$TGT"; \
	fi

.PHONY: build-shim build-shim-with-builder

build-shim:
	@set -e; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest (sidecar) ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  else \
	    echo "cargo-nextest not found in sidecar; running 'cargo test' ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo test $(ARGS); \
	  fi; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "Building aifo-shim with rustup (stable) ..."; \
	  rustup run stable cargo build --release --bin aifo-shim; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Building aifo-shim with local cargo ..."; \
	  cargo build --release --bin aifo-shim; \
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
	  $(RUST_BUILDER_IMAGE) cargo build --release --bin aifo-shim; \
	echo "Built (Linux target): $$(ls -1 target/*/release/aifo-shim 2>/dev/null || echo 'target/<triple>/release/aifo-shim')"

.PHONY: lint check test test-cargo test-legacy coverage coverage-html coverage-lcov

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
	  cargo clippy --workspace --all-features -- -D warnings; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if [ -n "$$RUSTUP_HOME" ] && [ -w "$$RUSTUP_HOME" ]; then rustup component add --toolchain stable rustfmt clippy >/dev/null 2>&1 || true; fi; \
	  rustup run stable cargo fmt -- --check || rustup run stable cargo fmt || cargo fmt; \
	  echo "Running cargo clippy (rustup stable) ..."; \
	  rustup run stable cargo clippy --workspace --all-features -- -D warnings || cargo clippy --workspace --all-features -- -D warnings; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (local cargo) ..."; \
	  cargo clippy --workspace --all-features -- -D warnings; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running lint inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; \
	      if cargo fmt --version >/dev/null 2>&1; then cargo fmt -- --check || cargo fmt; else echo "warning: cargo-fmt not installed in builder image; skipping format check" >&2; fi; \
	      cargo clippy --workspace --all-features -- -D warnings'; \
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
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  echo "Running cargo fmt --check (sidecar) ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (sidecar, excessive) ..."; \
	  cargo clippy --workspace --all-features -- -D warnings -D unsafe_code -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::dbg_macro -D clippy::print_stdout -D clippy::print_stderr -D clippy::await_holding_lock -D clippy::indexing_slicing; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if [ -n "$$RUSTUP_HOME" ] && [ -w "$$RUSTUP_HOME" ]; then rustup component add --toolchain stable rustfmt clippy >/dev/null 2>&1 || true; fi; \
	  rustup run stable cargo fmt -- --check || rustup run stable cargo fmt || cargo fmt; \
	  echo "Running cargo clippy (rustup stable, excessive) ..."; \
	  rustup run stable cargo clippy --workspace --all-features -- -D warnings -D unsafe_code -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::dbg_macro -D clippy::print_stdout -D clippy::print_stderr -D clippy::await_holding_lock -D clippy::indexing_slicing || cargo clippy --workspace --all-features -- -D warnings -D unsafe_code -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::dbg_macro -D clippy::print_stdout -D clippy::print_stderr -D clippy::await_holding_lock -D clippy::indexing_slicing; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "Running cargo fmt --check ..."; \
	  if cargo fmt --version >/dev/null 2>&1; then \
	    cargo fmt -- --check || cargo fmt; \
	  else \
	    echo "warning: cargo-fmt not installed; skipping format check" >&2; \
	  fi; \
	  echo "Running cargo clippy (local cargo, excessive) ..."; \
	  cargo clippy --workspace --all-features -- -D warnings -D unsafe_code -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::dbg_macro -D clippy::print_stdout -D clippy::print_stderr -D clippy::await_holding_lock -D clippy::indexing_slicing; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "Running lint inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; \
	      if cargo fmt --version >/dev/null 2>&1; then cargo fmt -- --check || cargo fmt; else echo "warning: cargo-fmt not installed in builder image; skipping format check" >&2; fi; \
	      cargo clippy --workspace --all-features -- -D warnings -D unsafe_code -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic -D clippy::dbg_macro -D clippy::print_stdout -D clippy::print_stderr -D clippy::await_holding_lock -D clippy::indexing_slicing'; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run lint." >&2; \
	  exit 1; \
	fi

check: lint test

test:
	@set -e; \
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
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest (sidecar) ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  else \
	    echo "cargo-nextest not found in sidecar; running 'cargo test' ..."; \
	    CARGO_TARGET_DIR=/var/tmp/aifo-target GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo test $(ARGS); \
	  fi; \
	elif command -v rustup >/dev/null 2>&1; then \
	  if rustup run stable cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest (rustup stable) ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 nice -n ${NICENESS_CARGO_NEXTEST} rustup run stable cargo nextest run $(ARGS_NEXTEST) $(ARGS) || GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "cargo-nextest not found locally; running inside $(RUST_BUILDER_IMAGE) (first run may install; slower) ..."; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" \
	      -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	      -v "$$HOME/.cargo/git:/root/.cargo/git" \
	      -v "$$PWD/target:/workspace/target" \
	      $(RUST_BUILDER_IMAGE) sh -lc 'nice -n $(NICENESS_CARGO_NEXTEST) cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS)'; \
	  else \
	    echo "cargo-nextest not found locally and docker unavailable; falling back to 'cargo test' via rustup ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo test $(ARGS); \
	  fi; \
	elif command -v cargo >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running cargo nextest ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS); \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "cargo-nextest not found locally; running inside $(RUST_BUILDER_IMAGE) (first run may install; slower) ..."; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" \
	      -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	      -v "$$HOME/.cargo/git:/root/.cargo/git" \
	      -v "$$PWD/target:/workspace/target" \
	      $(RUST_BUILDER_IMAGE) sh -lc 'cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS)'; \
	  else \
	    echo "cargo-nextest not found locally and docker unavailable; running 'cargo test' ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo test $(ARGS); \
	  fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "cargo/cargo-nextest not found locally; running tests inside $(RUST_BUILDER_IMAGE) ..."; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" \
	    -v "$$HOME/.cargo/registry:/root/.cargo/registry" \
	    -v "$$HOME/.cargo/git:/root/.cargo/git" \
	    -v "$$PWD/target:/workspace/target" \
	    $(RUST_BUILDER_IMAGE) sh -lc 'cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run $(ARGS_NEXTEST) $(ARGS)'; \
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
	    $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo test'; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

test-legacy: test-cargo

.PHONY: cov coverage-html coverage-lcov
cov: coverage-html coverage-lcov

coverage-html:
	@set -e; \
	mkdir -p build/coverage; rm -f build/coverage/*.profraw || true; \
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
	COV_ENV='CARGO_INCREMENTAL=0 RUSTFLAGS="-C instrument-coverage" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$(PWD)/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0'; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  echo "coverage-html (sidecar)"; \
	  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
	  if command -v grcov >/dev/null 2>&1; then \
	    grcov . --binary-path target -s . -t html --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o build/coverage/html; \
	  else \
	    echo "warning: grcov not found in sidecar; skipping html"; \
	  fi; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "coverage-html (rustup)"; \
	  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} rustup run stable cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS) || nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
	  if command -v grcov >/dev/null 2>&1; then \
	    grcov . --binary-path target -s . -t html --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o build/coverage/html; \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "grcov missing; running grcov in $(RUST_BUILDER_IMAGE)"; \
	    if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
	      echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
	      exit 1; \
	    fi; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
	      $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; grcov . --binary-path target -s . -t html --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/html'; \
	  else echo "error: grcov not found and no docker fallback"; exit 1; fi; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "coverage-html (cargo)"; \
	  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} ( cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked ); nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
	  if command -v grcov >/dev/null 2>&1; then \
	    grcov . --binary-path target -s . -t html --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o build/coverage/html; \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "grcov missing; running grcov in $(RUST_BUILDER_IMAGE)"; \
	    if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
	      echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
	      exit 1; \
	    fi; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	      -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
	      $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; grcov . --binary-path target -s . -t html --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/html'; \
	  else echo "error: grcov not found and no docker fallback"; exit 1; fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "coverage-html (docker $(RUST_BUILDER_IMAGE))"; \
	  if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
	    echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
	    exit 1; \
	  fi; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; export CARGO_INCREMENTAL=0 RUSTFLAGS="-C instrument-coverage"; export LLVM_PROFILE_FILE=/workspace/build/coverage/aifo-%p-%m.profraw; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS); grcov . --binary-path target -s . -t html --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/html'; \
	else echo "error: neither rustup/cargo nor docker found"; exit 1; fi; \
	echo "Wrote build/coverage/html (if grcov ran)."

coverage-lcov:
	@set -e; \
	mkdir -p build/coverage; rm -f build/coverage/*.profraw || true; \
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
	COV_ENV='CARGO_INCREMENTAL=0 RUSTFLAGS="-C instrument-coverage" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$(PWD)/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0'; \
	if [ -n "$$AIFO_EXEC_ID" ]; then \
	  echo "coverage-lcov (sidecar)"; \
	  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
	  if command -v grcov >/dev/null 2>&1; then \
	    grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o build/coverage/lcov.info; \
	  else echo "warning: grcov not found in sidecar; skipping lcov"; fi; \
	elif command -v rustup >/dev/null 2>&1; then \
	  echo "coverage-lcov (rustup)"; \
	  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} rustup run stable cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS) || nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
	  if command -v grcov >/dev/null 2>&1; then \
	    grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o build/coverage/lcov.info; \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "grcov missing; running grcov in $(RUST_BUILDER_IMAGE)"; \
	    if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
	      echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
	      exit 1; \
	    fi; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
	      $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/lcov.info'; \
	  else echo "error: grcov not found and no docker fallback"; exit 1; fi; \
	elif command -v cargo >/dev/null 2>&1; then \
	  echo "coverage-lcov (cargo)"; \
	  eval "$$COV_ENV LLVM_PROFILE_FILE=$(PWD)/build/coverage/aifo-%p-%m.profraw nice -n ${NICENESS_CARGO_NEXTEST} ( cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked ); nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS)"; \
	  if command -v grcov >/dev/null 2>&1; then \
	    grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o build/coverage/lcov.info; \
	  elif command -v docker >/dev/null 2>&1; then \
	    echo "grcov missing; running grcov in $(RUST_BUILDER_IMAGE)"; \
	    if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
	      echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
	      exit 1; \
	    fi; \
	    MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
	      $(RUST_BUILDER_IMAGE) sh -lc 'export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/lcov.info'; \
	  else echo "error: grcov not found and no docker fallback"; exit 1; fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "coverage-lcov (docker $(RUST_BUILDER_IMAGE))"; \
	  if ! docker image inspect $(RUST_BUILDER_IMAGE) >/dev/null 2>&1; then \
	    echo "Error: $(RUST_BUILDER_IMAGE) not present locally. Hint: make build-rust-builder"; \
	    exit 1; \
	  fi; \
	  MSYS_NO_PATHCONV=1 docker run $$DOCKER_PLATFORM_ARGS --rm \
	    -v "$$PWD:/workspace" -v "$$PWD/target:/workspace/target" -w /workspace \
	    $(RUST_BUILDER_IMAGE) sh -lc 'set -e; export CARGO_INCREMENTAL=0 RUSTFLAGS="-C instrument-coverage"; export LLVM_PROFILE_FILE=/workspace/build/coverage/aifo-%p-%m.profraw; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; nice -n ${NICENESS_CARGO_NEXTEST} cargo nextest run -j 1 --tests $(ARGS_NEXTEST) $(ARGS); grcov . --binary-path target -s . -t lcov --branch --ignore-not-existing --ignore "/*" $(ARGS_GRCOV) $(ARGS) -o /workspace/build/coverage/lcov.info'; \
	else echo "error: neither rustup/cargo nor docker found"; exit 1; fi; \
	echo "Wrote build/coverage/lcov.info (if grcov ran)."

.PHONY: test-proxy-smoke test-toolchain-live test-shim-embed test-proxy-unix test-toolchain-cpp test-proxy-errors
test-proxy-smoke:
	@echo "Running proxy smoke test (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test proxy_smoke -- --ignored

test-toolchain-live:
	@echo "Running live toolchain tests (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test toolchain_live -- --ignored

test-shim-embed:
	@echo "Running embedded shim presence test (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test shim_embed -- --ignored

test-proxy-unix:
	@set -e; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	if [ "$$OS" = "Linux" ]; then \
	  echo "Running unix-socket proxy test (ignored by default; Linux-only) ..."; \
	  CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test proxy_unix_socket -- --ignored; \
	else \
	  echo "Skipping unix-socket proxy test on $$OS; running TCP proxy smoke instead ..."; \
	  CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test proxy_smoke -- --ignored; \
	fi

test-proxy-errors:
	@echo "Running proxy error semantics tests (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test proxy_error_semantics -- --ignored

.PHONY: test-proxy-tcp
test-proxy-tcp:
	@echo "Running TCP streaming proxy test (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test proxy_streaming_tcp -- --ignored

.PHONY: test-acceptance-suite test-integration-suite test-e2e-suite

test-acceptance-suite:
	@set -e; \
	echo "Running acceptance test suite (ignored by default) via cargo nextest ..."; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	if [ "$$OS" = "Linux" ]; then \
	  EXPR='test(/^accept_/)' ; \
	else \
	  EXPR='test(/^accept_/) & !test(/_uds/)' ; \
	  echo "Skipping UDS acceptance test (non-Linux host)"; \
	fi; \
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run -j 1 --run-ignored ignored-only -E "$$EXPR" $(ARGS)

test-integration-suite:
	@set -e; \
	echo "Running integration/E2E test suite (ignored by default) via cargo nextest ..."; \
	OS="$$(uname -s 2>/dev/null || echo unknown)"; \
	if [ "$$OS" = "Linux" ]; then \
	  EXPR='test(/^test_proxy_/) | test(/^test_unix_socket_url_/) | test(/^test_dev_tool_routing_/) | test(/^test_tsc_/) | test(/^test_embedded_shim_/)' ; \
	else \
	  EXPR='test(/^test_proxy_/) | test(/^test_dev_tool_routing_/)' ; \
	fi; \
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo nextest run -j 1 --run-ignored ignored-only -E "$$EXPR" $(ARGS)
	@$(MAKE) test-toolchain-rust-e2e

test-e2e-suite:
	@echo "Running full ignored-by-default E2E suite (acceptance + integration) ..."
	$(MAKE) test-acceptance-suite
	$(MAKE) test-integration-suite

.PHONY: test-dev-tool-routing
test-dev-tool-routing:
	@echo "Running dev-tool routing tests (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test dev_tool_routing -- --ignored

.PHONY: test-tsc-resolution
test-tsc-resolution:
	@echo "Running TypeScript local tsc resolution test (ignored by default) ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test tsc_resolution -- --ignored

test-toolchain-cpp:
	@echo "Running c-cpp toolchain dry-run tests ..."
	CARGO_TARGET_DIR=/var/tmp/aifo-target cargo test --test toolchain_cpp

.PHONY: test-toolchain-rust test-toolchain-rust-e2e
test-toolchain-rust:
	@set -e; \
	if command -v rustup >/dev/null 2>&1; then \
	  rustup run stable cargo nextest -V >/dev/null 2>&1 || rustup run stable cargo install cargo-nextest --locked >/dev/null 2>&1 || true; \
	  echo "Running rust sidecar tests (unit/integration) via nextest ..."; \
	  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo nextest run -E 'test(/^toolchain_rust_/)' $(ARGS); \
	elif command -v cargo >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running rust sidecar tests (unit/integration) via nextest ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run -E 'test(/^toolchain_rust_/)' $(ARGS); \
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
	    $(RUST_BUILDER_IMAGE) sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest run -E 'test(/^toolchain_rust_/)' $(ARGS)"; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

test-toolchain-rust-e2e:
	@set -e; \
	if command -v rustup >/dev/null 2>&1; then \
	  rustup run stable cargo nextest -V >/dev/null 2>&1 || rustup run stable cargo install cargo-nextest --locked >/dev/null 2>&1 || true; \
	  echo "Running rust sidecar E2E tests (ignored by default) via nextest ..."; \
	  GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 rustup run stable cargo nextest run --run-ignored ignored-only -E 'test(/^toolchain_rust_/)' $(ARGS); \
	elif command -v cargo >/dev/null 2>&1; then \
	  if cargo nextest -V >/dev/null 2>&1; then \
	    echo "Running rust sidecar E2E tests (ignored by default) via nextest ..."; \
	    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$$PWD/ci/git-nosign.conf" GIT_TERMINAL_PROMPT=0 cargo nextest run --run-ignored ignored-only -E 'test(/^toolchain_rust_/)' $(ARGS); \
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
	    $(RUST_BUILDER_IMAGE) sh -lc "cargo nextest -V >/dev/null 2>&1 || cargo install cargo-nextest --locked; export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/workspace/ci/git-nosign.conf GIT_TERMINAL_PROMPT=0; cargo nextest run --run-ignored ignored-only -E 'test(/^toolchain_rust_/)' $(ARGS)"; \
	else \
	  echo "Error: neither rustup/cargo nor docker found; cannot run tests." >&2; \
	  exit 1; \
	fi

.PHONY: toolchain-cache-clear
toolchain-cache-clear:
	@echo "Purging toolchain cache volumes (cargo registry/git, node/npm, pip, ccache, go) ..."
	- docker volume rm -f aifo-cargo-registry aifo-cargo-git aifo-node-cache aifo-npm-cache aifo-pip-cache aifo-ccache aifo-go >/dev/null 2>&1 || true
	@echo "Done."

.PHONY: rebuild rebuild-coder rebuild-fat rebuild-codex rebuild-crush rebuild-aider rebuild-rust-builder
rebuild: rebuild-slim rebuild-fat rebuild-rust-builder rebuild-toolchain

rebuild-coder: rebuild-slim rebuild-fat rebuild-rust-builder

rebuild-fat: rebuild-codex rebuild-crush rebuild-aider rebuild-openhands rebuild-opencode rebuild-plandex

rebuild-codex:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex -t $(CODEX_IMAGE) -t "$${REG}$(CODEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex -t $(CODEX_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-crush:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush -t $(CRUSH_IMAGE) -t "$${REG}$(CRUSH_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush -t $(CRUSH_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-aider:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider -t $(AIDER_IMAGE) -t "$${REG}$(AIDER_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider -t $(AIDER_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-openhands:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target openhands -t $(OPENHANDS_IMAGE) -t "$${REG}$(OPENHANDS_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target openhands -t $(OPENHANDS_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-opencode:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target opencode -t $(OPENCODE_IMAGE) -t "$${REG}$(OPENCODE_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target opencode -t $(OPENCODE_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-plandex:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target plandex -t $(PLANDEX_IMAGE) -t "$${REG}$(PLANDEX_IMAGE)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target plandex -t $(PLANDEX_IMAGE) $(CA_SECRET) .; \
	fi

rebuild-rust-builder:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --target rust-builder -t $(RUST_BUILDER_IMAGE) -t "$${REG}$(RUST_BUILDER_IMAGE)" .; \
	else \
	  $(DOCKER_BUILD) --no-cache --build-arg REGISTRY_PREFIX="$$RP" --target rust-builder -t $(RUST_BUILDER_IMAGE) .; \
	fi

.PHONY: rebuild-slim rebuild-codex-slim rebuild-crush-slim rebuild-aider-slim
rebuild-slim: rebuild-codex-slim rebuild-crush-slim rebuild-aider-slim rebuild-openhands-slim rebuild-opencode-slim rebuild-plandex-slim

rebuild-codex-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${REG}$(CODEX_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex-slim -t $(CODEX_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-crush-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${REG}$(CRUSH_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush-slim -t $(CRUSH_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-aider-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${REG}$(AIDER_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider-slim -t $(AIDER_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-openhands-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) -t "$${REG}$(OPENHANDS_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target openhands-slim -t $(OPENHANDS_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-opencode-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
	if [ -n "$$REG" ]; then \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) -t "$${REG}$(OPENCODE_IMAGE_SLIM)" $(CA_SECRET) .; \
	else \
	  $(DOCKER_BUILD) --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target opencode-slim -t $(OPENCODE_IMAGE_SLIM) $(CA_SECRET) .; \
	fi

rebuild-plandex-slim:
	@RP=""; \
	echo "Checking reachability of https://repository.migros.net ..." ; \
	if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://repository.migros.net/v2/ >/dev/null 2>&1; then \
	  echo "repository.migros.net reachable via HTTPS; tagging image with registry prefix."; RP="repository.migros.net/"; \
	else \
	  echo "repository.migros.net not reachable via HTTPS; using Docker Hub (no prefix)."; \
	  if command -v curl >/dev/null 2>&1 && curl --connect-timeout 1 --max-time 2 -sSI -o /dev/null https://registry-1.docker.io/v2/ >/dev/null 2>&1; then \
	    echo "Docker Hub reachable via HTTPS; proceeding without registry prefix."; \
	  else \
	    echo "Error: Neither repository.migros.net nor Docker Hub is reachable via HTTPS; cannot rebuild images."; \
	    exit 1; \
	  fi; \
	fi; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	if [ -z "$$REG" ] && [ -n "$$RP" ]; then REG="$$RP"; fi; \
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
	docker rmi $(CODEX_IMAGE) $(CRUSH_IMAGE) $(AIDER_IMAGE) $(OPENHANDS_IMAGE) $(OPENCODE_IMAGE) $(PLANDEX_IMAGE) $(CODEX_IMAGE_SLIM) $(CRUSH_IMAGE_SLIM) $(AIDER_IMAGE_SLIM) $(OPENHANDS_IMAGE_SLIM) $(OPENCODE_IMAGE_SLIM) $(PLANDEX_IMAGE_SLIM) $(RUST_BUILDER_IMAGE) aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG) aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG) aifo-cpp-toolchain:latest 2>/dev/null || true; \
	docker rmi node:$(NODE_BASE_TAG) rust:$(RUST_BASE_TAG) 2>/dev/null || true; \
	REG="$${REGISTRY:-$${AIFO_CODER_REGISTRY_PREFIX}}"; \
	if [ -n "$$REG" ]; then case "$$REG" in */) ;; *) REG="$$REG/";; esac; fi; \
	RP="repository.migros.net/"; \
	if [ -n "$$REG" ]; then \
	  docker rmi "$${REG}$(CODEX_IMAGE)" "$${REG}$(CRUSH_IMAGE)" "$${REG}$(AIDER_IMAGE)" "$${REG}$(OPENHANDS_IMAGE)" "$${REG}$(OPENCODE_IMAGE)" "$${REG}$(PLANDEX_IMAGE)" "$${REG}$(CODEX_IMAGE_SLIM)" "$${REG}$(CRUSH_IMAGE_SLIM)" "$${REG}$(AIDER_IMAGE_SLIM)" "$${REG}$(OPENHANDS_IMAGE_SLIM)" "$${REG}$(OPENCODE_IMAGE_SLIM)" "$${REG}$(PLANDEX_IMAGE_SLIM)" "$${REG}$(RUST_BUILDER_IMAGE)" "$${REG}aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG)" "$${REG}aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG)" "$${REG}aifo-cpp-toolchain:latest" 2>/dev/null || true; \
	  docker rmi "$${REG}node:$(NODE_BASE_TAG)" "$${REG}rust:$(RUST_BASE_TAG)" 2>/dev/null || true; \
	fi; \
	if [ "$$RP" != "$$REG" ]; then \
	  docker rmi "$${RP}$(CODEX_IMAGE)" "$${RP}$(CRUSH_IMAGE)" "$${RP}$(AIDER_IMAGE)" "$${RP}$(OPENHANDS_IMAGE)" "$${RP}$(OPENCODE_IMAGE)" "$${RP}$(PLANDEX_IMAGE)" "$${RP}$(CODEX_IMAGE_SLIM)" "$${RP}$(CRUSH_IMAGE_SLIM)" "$${RP}$(AIDER_IMAGE_SLIM)" "$${RP}$(OPENHANDS_IMAGE_SLIM)" "$${RP}$(OPENCODE_IMAGE_SLIM)" "$${RP}$(PLANDEX_IMAGE_SLIM)" "$${RP}$(RUST_BUILDER_IMAGE)" "$${RP}aifo-rust-toolchain:$(RUST_TOOLCHAIN_TAG)" "$${RP}aifo-node-toolchain:$(NODE_TOOLCHAIN_TAG)" "$${RP}aifo-cpp-toolchain:latest" 2>/dev/null || true; \
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

.PHONY: hadolint
hadolint:
	@set -e; \
	if command -v hadolint >/dev/null 2>&1; then \
	  echo "Running hadolint on Dockerfile(s) ..."; \
	  hadolint Dockerfile || true; \
	  if [ -f toolchains/rust/Dockerfile ]; then hadolint toolchains/rust/Dockerfile || true; fi; \
	  if [ -f toolchains/cpp/Dockerfile ]; then hadolint toolchains/cpp/Dockerfile || true; fi; \
	elif command -v docker >/dev/null 2>&1; then \
	  echo "hadolint not found; using hadolint/hadolint container ..."; \
	  docker run --rm -i hadolint/hadolint < Dockerfile || true; \
	  if [ -f toolchains/rust/Dockerfile ]; then docker run --rm -i hadolint/hadolint < toolchains/rust/Dockerfile || true; fi; \
	  if [ -f toolchains/cpp/Dockerfile ]; then docker run --rm -i hadolint/hadolint < toolchains/cpp/Dockerfile || true; fi; \
	else \
	  echo "Error: hadolint not installed and docker unavailable."; \
	  echo "Install hadolint: https://github.com/hadolint/hadolint#install"; \
	  exit 1; \
	fi

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
