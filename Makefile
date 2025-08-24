.ONESHELL:

.PHONY: help
help:
	@echo ""
	@echo "aifo-coder - Makefile targets"
	@echo ""
	@echo "Variables:"
	@echo ""
	@echo "  IMAGE_PREFIX (aifo-coder) ... Image name prefix for per-agent images"
	@echo "  TAG (default: latest) ....... Tag for images"
	@echo ""
	@echo "  APPARMOR_PROFILE_NAME ....... Rendered AppArmor profile name (default: aifo-coder)"
	@echo "  DIST_DIR (dist) ............. Output directory for release archives"
	@echo "  BIN_NAME (aifo-coder) ....... Binary name used in release packaging"
	@echo "  VERSION ..................... Version inferred from Cargo.toml or git describe"
	@echo "  RELEASE_TARGETS ............. Space-separated Rust targets for 'make release-for-target' (overrides auto-detect)"
	@echo "  CONTAINER ................... Container name for docker-enter (optional)"
	@echo "  CODEX_IMAGE ................. Full image ref for Codex ($${IMAGE_PREFIX}-codex:$${TAG})"
	@echo "  CRUSH_IMAGE ................. Full image ref for Crush ($${IMAGE_PREFIX}-crush:$${TAG})"
	@echo "  AIDER_IMAGE ................. Full image ref for Aider ($${IMAGE_PREFIX}-aider:$${TAG})"
	@echo ""
	@echo "  APP_NAME .................... App bundle name for macOS .app (default: aifo-coder)"
	@echo "  APP_BUNDLE_ID .............. macOS bundle identifier (default: ch.migros.aifo-coder)"
	@echo "  APP_ICON .................... Path to a .icns icon to include in the .app (optional)"
	@echo "  DMG_NAME .................... DMG filename base (default: $${APP_NAME}-$${VERSION})"
	@echo "  SIGN_IDENTITY ............... macOS code signing identity (default: Migros AI Foundation Code Signer)"
	@echo "  NOTARY_PROFILE .............. Keychain profile for xcrun notarytool (optional)"
	@echo "  DMG_BG ...................... Background image for DMG (default: images/aifo-sticker-1024x1024-web.jpg)"
	@echo ""
	@echo "Install paths (for 'make install'):"
	@echo ""
	@echo "  PREFIX (/usr/local) ......... Install prefix"
	@echo "  DESTDIR () .................. Staging root for packaging"
	@echo "  BIN_DIR ($${PREFIX}/bin) ..... Binary install dir"
	@echo "  MAN_DIR ($${PREFIX}/share/man) Manpages root"
	@echo "  MAN1_DIR ($${MAN_DIR}/man1) .. Section 1 manpages"
	@echo "  DOC_DIR ($${PREFIX}/share/doc/$${BIN_NAME}) ......... Documentation dir"
	@echo "  EXAMPLES_DIR ($${DOC_DIR}/examples) ................. Examples dir"
	@echo ""
	@echo "Release and cross-compile:"
	@echo ""
	@echo "  release ..................... Aggregate: build launcher, mac .app + .dmg, and both mac (host) and Linux"
	@echo "  release-for-mac ............. Build release for the current host (calls release-for-target)"
	@echo "  release-for-linux ........... Build Linux release (RELEASE_TARGETS=x86_64-unknown-linux-gnu)"
	@echo "                                Hints: set RELEASE_TARGETS='x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu'"
	@echo "  release-for-target .......... Build release archives into dist/ for targets in RELEASE_TARGETS or host default"
	@echo ""
	@echo "Build launcher:"
	@echo ""
	@echo "  build-launcher .............. Build the Rust host launcher (cargo build --release)"
	@echo "  build-app ................... Build macOS .app bundle into dist/ (Darwin hosts only)"
	@echo "  build-dmg ................... Build macOS .dmg image from the .app (Darwin hosts only)"
	@echo ""
	@echo "Install:"
	@echo ""
	@echo "  install ..................... Install binary, man page, LICENSE/README and examples, then build Docker images locally"
	@echo ""
	@echo "Build images:"
	@echo ""
	@echo "  build ....................... Build both slim and fat images (all agents)"
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
	@echo "Rebuild images:"
	@echo ""
	@echo "  rebuild ..................... Rebuild both slim and fat images without cache"
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
	@echo "Rebuild existing images by prefix:"
	@echo ""
	@echo "  rebuild-existing ............ Rebuild any existing local images with IMAGE_PREFIX (using cache)"
	@echo "  rebuild-existing-nocache .... Same, but without cache"
	@echo ""
	@echo "Utilities:"
	@echo ""
	@echo "  clean ....................... Remove built images (ignores errors if not present)"
	@echo "  loc ......................... Count lines of source code (Rust, Shell, Dockerfiles, Makefiles, YAML/TOML/JSON, Markdown)"
	@echo "  docker-images ............... Show the available images in the local Docker registry"
	@echo "  docker-enter ................ Enter a running container via docker exec with GPG runtime prepared"
	@echo "                                Use CONTAINER=name to choose a specific container; default picks first matching prefix."
	@echo "  checksums ................... Generate dist/SHA256SUMS.txt for current artifacts"
	@echo "  sbom ........................ Generate CycloneDX SBOM into dist/SBOM.cdx.json (requires cargo-cyclonedx)"
	@echo "  scrub-coauthors ............. Rewrite history to remove the aider co-author line from all commit messages"
	@echo "                                WARNING: This rewrites history. Ensure you have backups and will force-push."
	@echo ""
	@echo "  gpg-show-config ............. Show current git GPG signing-related configuration"
	@echo "  git-check-signatures ........ Show commit signature status (git log %h %G? %s)"
	@echo ""
	@echo "  gpg-enable-signing .......... Re-enable GPG signing for commits and tags in this repo"
	@echo "  gpg-disable-signing ......... Disable GPG signing for commits and tags in this repo (use if commits fail to sign)"
	@echo "  gpg-disable-signing-global .. Disable GPG signing globally (in your ~/.gitconfig)"
	@echo "  gpg-unset-signing ........... Unset local signing config for this repo (return to defaults)"
	@echo ""
	@echo "  git-commit-no-sign .......... Commit staged changes without GPG signing (MESSAGE='your message')"
	@echo "  git-amend-no-sign ........... Amend the last commit without GPG signing"
	@echo "  git-commit-no-sign-all ...... Stage all and commit without signing (MESSAGE='your message' optional)"
	@echo ""
	@echo "AppArmor (security) profile:"
	@echo
	@echo "  apparmor .................... Generate build/apparmor/$${APPARMOR_PROFILE_NAME} from template"
	@echo ""
	@echo "  apparmor-load-colima ........ Load the generated profile directly into the Colima VM"
	@echo "  apparmor-log-colima ......... Stream AppArmor logs (Colima VM or local Linux) into build/logs/apparmor.log"
	@echo ""
	@echo "Tip: Override variables inline, e.g.: make TAG=dev build-codex"
	@echo ""
	@echo "Usage:"
	@echo ""
	@echo "   make IMAGE_PREFIX=myrepo/aifo-coder TAG=v1 build"
	@echo ""
	@echo "   Load AppArmor policy into Colima VM (macOS):"
	@echo "   colima ssh -- sudo apparmor_parser -r -W \"$$PWD/build/apparmor/$${APPARMOR_PROFILE_NAME}\""
	@echo ""

# Build one image per agent with shared base layers for maximal cache reuse.

IMAGE_PREFIX ?= aifo-coder
TAG ?= latest
# Set to 1 to keep apt/procps in final images (default drops them in final stages)
KEEP_APT ?= 0

CODEX_IMAGE ?= $(IMAGE_PREFIX)-codex:$(TAG)
CRUSH_IMAGE ?= $(IMAGE_PREFIX)-crush:$(TAG)
AIDER_IMAGE ?= $(IMAGE_PREFIX)-aider:$(TAG)
CODEX_IMAGE_SLIM ?= $(IMAGE_PREFIX)-codex-slim:$(TAG)
CRUSH_IMAGE_SLIM ?= $(IMAGE_PREFIX)-crush-slim:$(TAG)
AIDER_IMAGE_SLIM ?= $(IMAGE_PREFIX)-aider-slim:$(TAG)

.PHONY: build build-fat build-codex build-crush build-aider build-launcher
build-fat: build-codex build-crush build-aider

build: build-slim build-fat

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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) -t "$${RP}$(CODEX_IMAGE)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex -t $(CODEX_IMAGE) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush -t $(CRUSH_IMAGE) -t "$${RP}$(CRUSH_IMAGE)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush -t $(CRUSH_IMAGE) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider -t $(AIDER_IMAGE) -t "$${RP}$(AIDER_IMAGE)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider -t $(AIDER_IMAGE) .; \
	fi

.PHONY: build-slim build-codex-slim build-crush-slim build-aider-slim
build-slim: build-codex-slim build-crush-slim build-aider-slim

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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${RP}$(CODEX_IMAGE_SLIM)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target codex-slim -t $(CODEX_IMAGE_SLIM) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${RP}$(CRUSH_IMAGE_SLIM)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target crush-slim -t $(CRUSH_IMAGE_SLIM) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${RP}$(AIDER_IMAGE_SLIM)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --target aider-slim -t $(AIDER_IMAGE_SLIM) .; \
	fi

build-launcher:
	cargo build --release

.PHONY: test
test:
	cargo test

.PHONY: rebuild rebuild-fat rebuild-codex rebuild-crush rebuild-aider
rebuild-fat: rebuild-codex rebuild-crush rebuild-aider

rebuild: rebuild-slim rebuild-fat

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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex -t $(CODEX_IMAGE) -t "$${RP}$(CODEX_IMAGE)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex -t $(CODEX_IMAGE) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush -t $(CRUSH_IMAGE) -t "$${RP}$(CRUSH_IMAGE)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush -t $(CRUSH_IMAGE) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider -t $(AIDER_IMAGE) -t "$${RP}$(AIDER_IMAGE)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider -t $(AIDER_IMAGE) .; \
	fi

.PHONY: rebuild-slim rebuild-codex-slim rebuild-crush-slim rebuild-aider-slim
rebuild-slim: rebuild-codex-slim rebuild-crush-slim rebuild-aider-slim

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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex-slim -t $(CODEX_IMAGE_SLIM) -t "$${RP}$(CODEX_IMAGE_SLIM)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target codex-slim -t $(CODEX_IMAGE_SLIM) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush-slim -t $(CRUSH_IMAGE_SLIM) -t "$${RP}$(CRUSH_IMAGE_SLIM)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target crush-slim -t $(CRUSH_IMAGE_SLIM) .; \
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
	if [ -n "$$RP" ]; then \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider-slim -t $(AIDER_IMAGE_SLIM) -t "$${RP}$(AIDER_IMAGE_SLIM)" .; \
	else \
	  docker build --build-arg REGISTRY_PREFIX="$$RP" --build-arg KEEP_APT="$(KEEP_APT)" --no-cache --target aider-slim -t $(AIDER_IMAGE_SLIM) .; \
	fi

# Rebuild all existing local images for this prefix (all tags) using cache
.PHONY: rebuild-existing
rebuild-existing:
	@set -e; \
	prefix="$(IMAGE_PREFIX)"; \
	imgs=$$(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E "^$${prefix}-(codex|crush|aider):" || true); \
	if [ -z "$$imgs" ]; then echo "No existing images found for prefix $$prefix"; exit 0; fi; \
	for img in $$imgs; do \
	  repo="$${img%%:*}"; \
	  base="$$(basename "$$repo")"; \
	  agent="$$(echo "$$base" | sed -E 's/.*-//')"; \
	  echo "Rebuilding $$img (target=$$agent) ..."; \
	  docker build --target "$$agent" -t "$$img" .; \
	done

# Rebuild all existing local images for this prefix (all tags) without cache
.PHONY: rebuild-existing-nocache
rebuild-existing-nocache:
	@set -e; \
	prefix="$(IMAGE_PREFIX)"; \
	imgs=$$(docker images --format '{{.Repository}}:{{.Tag}}' | grep -E "^$${prefix}-(codex|crush|aider):" || true); \
	if [ -z "$$imgs" ]; then echo "No existing images found for prefix $$prefix"; exit 0; fi; \
	for img in $$imgs; do \
	  repo="$${img%%:*}"; \
	  base="$$(basename "$$repo")"; \
	  agent="$$(echo "$$base" | sed -E 's/.*-//')"; \
	  echo "Rebuilding (no cache) $$img (target=$$agent) ..."; \
	  docker build --no-cache --target "$$agent" -t "$$img" .; \
	done

.PHONY: clean
clean:
	-@docker rmi $(CODEX_IMAGE) $(CRUSH_IMAGE) $(AIDER_IMAGE) $(CODEX_IMAGE_SLIM) $(CRUSH_IMAGE_SLIM) $(AIDER_IMAGE_SLIM) 2>/dev/null || true

# AppArmor profile generation (for Docker containers)
APPARMOR_PROFILE_NAME ?= aifo-coder

.PHONY: apparmor
apparmor:
	mkdir -p build/apparmor
	sed -e 's/__PROFILE_NAME__/$(APPARMOR_PROFILE_NAME)/g' apparmor/aifo-coder.apparmor.tpl > build/apparmor/$(APPARMOR_PROFILE_NAME)
	@echo "Wrote build/apparmor/$(APPARMOR_PROFILE_NAME)"
	@echo "Load into AppArmor on a Linux host with:"
	@echo "  sudo apparmor_parser -r -W build/apparmor/$(APPARMOR_PROFILE_NAME)"
	@echo "Load into Colima's VM (macOS) with:"
	@echo "  colima ssh -- sudo apparmor_parser -r -W \"$(PWD)/build/apparmor/$(APPARMOR_PROFILE_NAME)\""

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

.PHONY: docker-enter
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

.PHONY: git-check-signatures
git-check-signatures:
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
VERSION ?= $(shell sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)
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
	if command -v rustup >/dev/null 2>&1; then \
	  BUILD_HOST="rustup run $$CHANNEL cargo build --release --target"; \
	else \
	  BUILD_HOST="cargo build --release --target"; \
	fi; \
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
	  HOST_OK=0; \
	  if [ -n "$$RUSTC_HOST" ] && [ "$$t" = "$$RUSTC_HOST" ]; then HOST_OK=1; fi; \
	  if [ "$$HOST_OK" -eq 1 ]; then \
	    echo "Building with cargo for host target $$t ..."; \
	    $$BUILD_HOST "$$t" || echo "Warning: build failed for $$t"; \
	  elif command -v rustup >/dev/null 2>&1; then \
	    echo "Ensuring rustup target $$t is installed ..."; \
	    rustup target add "$$t" || true; \
	    echo "Building with cargo for $$t ..."; \
	    $$BUILD_HOST "$$t" || echo "Warning: build failed for $$t"; \
	  else \
	    echo "rustup not available and target != host; skipping $$t"; \
	  fi; \
	done; \
	[ -n "$$BIN" ] || BIN="$$(sed -n 's/^name[[:space:]]*=[[:space:]]*\"\(.*\)\"/\1/p' Cargo.toml | head -n1)"; \
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
	# Generate checksums for archives (tar.gz, dmg)
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
	# Generate SBOM via cargo-cyclonedx (this tool writes <package>.cdx.{json,xml} into the project root)
	if command -v cargo >/dev/null 2>&1 && cargo cyclonedx -h >/dev/null 2>&1; then \
	  PKG="$$(sed -n 's/^name[[:space:]]*=[[:space:]]*\"\(.*\)\"/\1/p' Cargo.toml | head -n1)"; \
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

# Convenience targets wrapping release-for-target
release-for-mac: release-for-target

release-for-linux:
	@$(MAKE) RELEASE_TARGETS=x86_64-unknown-linux-gnu release-for-target

# Build both mac (host) and Linux, and also build launcher and mac app/dmg
release: rebuild build-launcher build-app build-dmg release-for-mac release-for-linux

.PHONY: install
install: build build-launcher
	@set -e; \
	BIN="$(BIN_NAME)"; \
	BINPATH="target/release/$$BIN"; \
	BIN_US="$$(printf '%s' "$$BIN" | tr '-' '_')"; \
	[ -f "$$BINPATH" ] || BINPATH="target/release/$$BIN_US"; \
	if [ ! -f "$$BINPATH" ]; then echo "Error: binary not found at $$BINPATH. Build failed?" >&2; exit 1; fi; \
	install -d -m 0755 "$(BIN_DIR)" "$(MAN1_DIR)" "$(DOC_DIR)"; \
	install -m 0755 "$$BINPATH" "$(BIN_DIR)/$$BIN"; \
	if [ -f man/$$BIN.1 ]; then \
	  if command -v gzip >/dev/null 2>&1; then \
	    TMP="$$(mktemp)"; cp "man/$$BIN.1" "$$TMP"; gzip -c "$$TMP" > "$(MAN1_DIR)/$$BIN.1.gz"; rm -f "$$TMP"; \
	  else \
	    install -m 0644 "man/$$BIN.1" "$(MAN1_DIR)/$$BIN.1"; \
	  fi; \
	fi; \
	[ -f README.md ] && install -m 0644 README.md "$(DOC_DIR)/" || true; \
	[ -f LICENSE ] && install -m 0644 LICENSE "$(DOC_DIR)/" || true; \
	if [ -d examples ]; then \
	  install -d -m 0755 "$(EXAMPLES_DIR)"; \
	  cp -a examples/. "$(EXAMPLES_DIR)/"; \
	  chmod -R u=rwX,go=rX "$(EXAMPLES_DIR)" || true; \
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
	  PKG="$$(sed -n 's/^name[[:space:]]*=[[:space:]]*\"\(.*\)\"/\1/p' Cargo.toml | head -n1)"; \
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
	echo "\nCounting lines of source in repository...\n"; \
	count() { \
	  pat="$$1"; \
	  eval "find . \\( -path './.git' -o -path './target' -o -path './dist' -o -path './build' -o -path './node_modules' \\) -prune -o -type f \\( $${pat} \\) -print0" \
	    | xargs -0 wc -l 2>/dev/null | awk 'END{print ($$1+0)}'; \
	}; \
	rust=$$(count "-name '*.rs'"); \
	shell=$$(count "-name '*.sh' -o -name '*.bash' -o -name '*.zsh'"); \
	makef=$$(count "-name 'Makefile' -o -name '*.mk'"); \
	docker=$$(count "-name 'Dockerfile' -o -name '*.dockerfile'"); \
	yaml=$$(count "-name '*.yml' -o -name '*.yaml'"); \
	toml=$$(count "-name '*.toml'"); \
	json=$$(count "-name '*.json'"); \
	md=$$(count "-name '*.md'"); \
	other=$$(count "-name '*.conf'"); \
	total=$$((rust+shell+makef+docker+yaml+toml+json+md+other)); \
	printf "Lines of code (excluding .git, target, dist, build, node_modules):\n\n"; \
	printf "  Rust (.rs):      %8d\n" "$$rust"; \
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

.PHONY: build-app build-dmg
ifeq ($(shell uname -s),Darwin)

build-app:
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
	SIGN_ID="$(SIGN_IDENTITY)"; \
	if [ -n "$$SIGN_ID" ] && command -v codesign >/dev/null 2>&1; then \
	  echo "Signing app with identity: $$SIGN_ID"; \
	  codesign --force --deep --options runtime --timestamp -s "$$SIGN_ID" "$$APPROOT"; \
	  codesign --verify --deep --strict "$$APPROOT" || { echo "codesign verification failed"; exit 1; }; \
	else \
	  echo "Skipping code signing (SIGN_IDENTITY empty or codesign not available)"; \
	fi; \
	echo "Built $$APPROOT"; \
	)

build-dmg: build-app
	@( \
	BIN="$(BIN_NAME)"; \
	VERSION="$(VERSION)"; \
	DIST="$(DIST_DIR)"; \
	APP="$(APP_NAME)"; \
	DMG="$(DMG_NAME)"; \
	APPROOT="$$DIST/$$APP.app"; \
	if [ ! -d "$$APPROOT" ]; then echo "App bundle not found at $$APPROOT; run 'make build-app' first." >&2; exit 1; fi; \
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
	# Create temporary read-write DMG to customize Finder view \
	TMP_DMG="$$DIST/.tmp-$$DMG.dmg"; \
	MNT="$$DIST/.mnt-$$APP"; \
	rm -f "$$TMP_DMG"; \
	hdiutil create -ov -fs HFS+J -srcfolder "$$STAGE" -volname "$$APP" -format UDRW "$$TMP_DMG"; \
	rm -rf "$$STAGE"; \
	mkdir -p "$$MNT"; \
	hdiutil attach -readwrite -noverify -noautoopen -mountpoint "$$MNT" "$$TMP_DMG" >/dev/null; \
	# Configure Finder window via AppleScript (best-effort) \
	if command -v osascript >/dev/null 2>&1; then \
	  AS="tell application \"Finder\" \
tell disk \"$$APP\" \
open \
set current view of container window to icon view \
set toolbar visible of container window to false \
set statusbar visible of container window to false \
set bounds of container window to {100, 100, 680, 480} \
set opts to the icon view options of container window \
set arrangement of opts to not arranged \
set icon size of opts to 96 \
"; \
	  if [ -n "$$BG_NAME" ]; then \
	    AS="$$AS set background picture of opts to POSIX file \"$$MNT/.background/$$BG_NAME\" \
"; \
	  fi; \
	  AS="$$AS \
delay 1 \
set position of item \"$$APP.app\" of container window to {120, 260} \
set position of item \"Applications\" of container window to {360, 260} \
close \
open \
update without registering applications \
delay 1 \
end tell \
end tell"; \
	  osascript -e "$$AS" || true; \
	else \
	  echo "osascript not available; skipping Finder customization." >&2; \
	fi; \
	# Detach and convert to compressed DMG \
	hdiutil detach "$$MNT" -quiet || hdiutil detach "$$MNT" -force -quiet || true; \
	rm -rf "$$MNT"; \
	hdiutil convert "$$TMP_DMG" -format UDZO -imagekey zlib-level=9 -ov -o "$$DMG_PATH" >/dev/null; \
	rm -f "$$TMP_DMG"; \
	# Sign DMG if identity provided \
	SIGN_ID="$(SIGN_IDENTITY)"; \
	if [ -n "$$SIGN_ID" ] && command -v codesign >/dev/null 2>&1; then \
	  echo "Signing DMG with identity: $$SIGN_ID"; \
	  codesign --force --timestamp -s "$$SIGN_ID" "$$DMG_PATH"; \
	else \
	  echo "Skipping DMG signing (SIGN_IDENTITY empty or codesign not available)"; \
	fi; \
	# Notarize and staple if notary profile configured \
	NOTARY="$(NOTARY_PROFILE)"; \
	if [ -n "$$NOTARY" ] && command -v xcrun >/dev/null 2>&1; then \
	  if xcrun notarytool --help >/dev/null 2>&1; then \
	    echo "Submitting $$DMG_PATH for notarization with profile '$$NOTARY' (this may take a while) ..."; \
	    xcrun notarytool submit "$$DMG_PATH" --keychain-profile "$$NOTARY" --wait || { echo "Notarization failed" >&2; exit 1; }; \
	    echo "Stapling notarization ticket ..."; \
	    xcrun stapler staple "$$DMG_PATH" || true; \
	    xcrun stapler staple "$$APPROOT" || true; \
	  else \
	    echo "xcrun notarytool not available; skipping notarization." >&2; \
	  fi; \
	else \
	  echo "NOTARY_PROFILE not set or xcrun not available; skipping notarization." >&2; \
	fi; \
	echo "Wrote $$DMG_PATH"; \
	)

else

build-app:
	@echo "build-app is only supported on macOS (Darwin) hosts." >&2; exit 1

build-dmg:
	@echo "build-dmg is only supported on macOS (Darwin) hosts." >&2; exit 1

endif
