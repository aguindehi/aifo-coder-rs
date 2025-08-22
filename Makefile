.ONESHELL:

.PHONY: help
help:
	@echo ""
	@echo "aifo-coder Makefile - targets"
	@echo ""
	@echo "Variables:"
	@echo ""
	@echo "  IMAGE_PREFIX (default: aifo-coder)  - Image name prefix for per-agent images"
	@echo "  TAG (default: latest)              - Tag for images"
	@echo ""
	@echo "Build images:"
	@echo ""
	@echo "  build                              - Build all per-agent images (codex, crush, aider)"
	@echo "  build-launcher                     - Build the Rust host launcher (cargo build --release)"
	@echo "  build-codex                        - Build only the Codex image ($${IMAGE_PREFIX}-codex:$${TAG})"
	@echo "  build-crush                        - Build only the Crush image ($${IMAGE_PREFIX}-crush:$${TAG})"
	@echo "  build-aider                        - Build only the Aider image ($${IMAGE_PREFIX}-aider:$${TAG})"
	@echo ""
	@echo "Rebuild images:"
	@echo ""
	@echo "  rebuild                            - Rebuild all images without cache"
	@echo "  rebuild-codex                      - Rebuild only the Codex image without cache"
	@echo "  rebuild-crush                      - Rebuild only the Crush image without cache"
	@echo "  rebuild-aider                      - Rebuild only the Aider image without cache"
	@echo ""
	@echo "Rebuild existing images by prefix:"
	@echo ""
	@echo "  rebuild-existing                   - Rebuild any existing local images with IMAGE_PREFIX (using cache)"
	@echo "  rebuild-existing-nocache           - Same, but without cache"
	@echo ""
	@echo "Utilities:"
	@echo ""
	@echo "  clean                              - Remove built images (ignores errors if not present)"
	@echo "  scrub-coauthors                    - Rewrite history to remove the aider co-author line from all commit messages"
	@echo "                                       WARNING: This rewrites history. Ensure you have backups and will force-push."
	@echo "  gpg-disable-signing                - Disable GPG signing for commits and tags in this repo (use if commits fail to sign)"
	@echo "  gpg-enable-signing                 - Re-enable GPG signing for commits and tags in this repo"
	@echo "  gpg-show-config                    - Show current git GPG signing-related configuration"
	@echo "  gpg-disable-signing-global         - Disable GPG signing globally (in your ~/.gitconfig)"
	@echo "  gpg-unset-signing                  - Unset local signing config for this repo (return to defaults)"
	@echo "  git-check-signatures               - Show commit signature status (git log %h %G? %s)"
	@echo "  git-commit-no-sign                 - Commit staged changes without GPG signing (MESSAGE='your message')"
	@echo "  git-amend-no-sign                  - Amend the last commit without GPG signing"
	@echo "  git-commit-no-sign-all             - Stage all and commit without signing (MESSAGE='your message' optional)"
	@echo "  docker-enter                       - Enter a running container via docker exec with GPG runtime prepared"
	@echo "  release                            - Build multi-platform release archives into dist/"
	@echo ""
	@echo "AppArmor (security) profile:"
	@echo
	@echo "  apparmor                           - Generate build/apparmor/$${APPARMOR_PROFILE_NAME} from template"
	@echo ""
	@echo "  apparmor-load-colima  - Load the generated profile directly into the Colima VM"
	@echo "  apparmor-log-colima   - Stream AppArmor logs (Colima VM or local Linux) into build/logs/apparmor.log"
	@echo ""
	@echo "Tip: Override variables inline, e.g.: make TAG=dev build-codex"
	@echo ""
	@echo "Usage:"
	@echo ""
	@echo "   make IMAGE_PREFIX=myrepo/aifo-coder TAG=v1 build"
	@echo "   Then load on Linux:"
	@echo "   sudo apparmor_parser -r -W build/apparmor/$${APPARMOR_PROFILE_NAME}"
	@echo "   Or load into Colima VM (macOS):"
	@echo "   colima ssh -- sudo apparmor_parser -r -W \"$$PWD/build/apparmor/$${APPARMOR_PROFILE_NAME}\""
	@echo ""

# Build one image per agent with shared base layers for maximal cache reuse.

IMAGE_PREFIX ?= aifo-coder
TAG ?= latest

CODEX_IMAGE ?= $(IMAGE_PREFIX)-codex:$(TAG)
CRUSH_IMAGE ?= $(IMAGE_PREFIX)-crush:$(TAG)
AIDER_IMAGE ?= $(IMAGE_PREFIX)-aider:$(TAG)

.PHONY: build build-codex build-crush build-aider build-launcher
build: build-codex build-crush build-aider

build-codex:
	docker build --target codex -t $(CODEX_IMAGE) .

build-crush:
	docker build --target crush -t $(CRUSH_IMAGE) .

build-aider:
	docker build --target aider -t $(AIDER_IMAGE) .

build-launcher:
	cargo build --release

.PHONY: rebuild rebuild-codex rebuild-crush rebuild-aider
rebuild: rebuild-codex rebuild-crush rebuild-aider

rebuild-codex:
	docker build --no-cache --target codex -t $(CODEX_IMAGE) .

rebuild-crush:
	docker build --no-cache --target crush -t $(CRUSH_IMAGE) .

rebuild-aider:
	docker build --no-cache --target aider -t $(AIDER_IMAGE) .

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
	-@docker rmi $(CODEX_IMAGE) $(CRUSH_IMAGE) $(AIDER_IMAGE) 2>/dev/null || true

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

# Build release binaries and package archives for macOS and Linux (Ubuntu/Arch)
# Requires: cargo (and optionally cross), appropriate targets installed for non-native builds
.PHONY: release
release:
	@set -e; \
	BIN="$(BIN_NAME)"; \
	VERSION="$(VERSION)"; \
	DIST="$(DIST_DIR)"; \
	mkdir -p "$$DIST"; \
	echo "Building release version: $$VERSION"; \
	rm -f Cargo.lock || true; \
	PATH="$$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$$PATH"; \
	CROSS_BIN=""; \
	if [ -x "/Users/m0565460/.cargo/bin/cross" ]; then CROSS_BIN="/Users/m0565460/.cargo/bin/cross"; \
	elif [ -x "$$HOME/.cargo/bin/cross" ]; then CROSS_BIN="$$HOME/.cargo/bin/cross"; \
	elif command -v cross >/dev/null 2>&1; then CROSS_BIN="cross"; fi; \
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
	  if [ -n "$$CROSS_BIN" ] && printf "%s" "$$t" | grep -Eq -- '-linux-gnu|-linux-musl'; then \
	    VARNAME="$$(echo "$$t" | tr '[:lower:]-' '[:upper:]_')"; \
	    IMAGE="repository.migros.net/ghcr.io/cross-rs/$$t:edge"; \
	    RUNTIME="$${CONTAINER_RUNTIME:-docker}"; \
	    if ! command -v "$$RUNTIME" >/dev/null 2>&1; then \
	      echo "No container runtime found; install Docker or Podman to cross-compile $$t"; \
	      continue; \
	    fi; \
	    case "$$t" in \
	      x86_64-*) PLATFORM="linux/amd64" ;; \
	      aarch64-*|arm64-*) PLATFORM="linux/arm64" ;; \
	      armv7-*) PLATFORM="linux/arm/v7" ;; \
	      i686-*) PLATFORM="linux/386" ;; \
	      riscv64-*) PLATFORM="linux/riscv64" ;; \
	      *) PLATFORM="" ;; \
	    esac; \
	    PLATFORM_ARG=""; [ -n "$$PLATFORM" ] && PLATFORM_ARG="--platform $$PLATFORM"; \
	    echo "Building for $$t inside $$IMAGE with $$RUNTIME $$PLATFORM_ARG (no host rustup/toolchains) ..."; \
	    "$$RUNTIME" run $$PLATFORM_ARG --rm -v "$$(pwd)":/project -w /project "$$IMAGE" \
	      bash -lc "set -e; export HOME=\$${HOME:-/root}; export CARGO_HOME=\$${CARGO_HOME:-\$${HOME}/.cargo}; export RUSTUP_HOME=\$${RUSTUP_HOME:-\$${HOME}/.rustup}; export PATH=\$${CARGO_HOME}/bin:/usr/local/cargo/bin:/usr/local/bin:/usr/bin:/bin:\$${PATH}; if ! command -v curl >/dev/null 2>&1; then if command -v apt-get >/dev/null 2>&1; then apt-get update && apt-get install -y --no-install-recommends curl ca-certificates && update-ca-certificates || true; elif command -v dnf >/dev/null 2>&1; then dnf install -y curl ca-certificates || true; elif command -v apk >/dev/null 2>&1; then apk add --no-cache curl ca-certificates || true; fi; fi; if ! command -v cargo >/dev/null 2>&1; then if command -v rustup >/dev/null 2>&1; then rustup set profile minimal || true; rustup toolchain install stable -c rust-src; rustup default stable; else curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable; . \"\$${HOME}/.cargo/env\"; fi; fi; if command -v rustup >/dev/null 2>&1; then rustup target add '$$t' || true; fi; CARGOBIN=\"\$$(command -v cargo || echo \"\$${CARGO_HOME}/bin/cargo\" || echo \"/root/.cargo/bin/cargo\")\"; \"\$${CARGOBIN}\" --version; \"\$${CARGOBIN}\" build --release --target '$$t'" || echo "Warning: build failed for $$t"; \
	  elif [ "$$HOST_OK" -eq 1 ]; then \
	    echo "Building with cargo for host target $$t ..."; $$BUILD_HOST "$$t" || echo "Warning: build failed for $$t"; \
	  elif command -v rustup >/dev/null 2>&1 && rustup target list --installed | grep -qx "$$t"; then \
	    echo "Building with cargo for $$t ..."; $$BUILD_HOST "$$t" || echo "Warning: build failed for $$t"; \
	  else \
	    echo "Target $$t not installed. Skipping (install with: rustup target add $$t)"; \
	    continue; \
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
	  if [ "$$OS" = "linux" ]; then \
	    for distro in ubuntu arch; do \
	      cp "$$D/$$PKG.tar.gz" "$$D/$$BIN-$$V-$$distro-$$ARCH.tar.gz"; \
	      chmod 0644 "$$D/$$BIN-$$V-$$distro-$$ARCH.tar.gz" || true; \
	      echo "Wrote $$D/$$BIN-$$V-$$distro-$$ARCH.tar.gz"; \
	    done; \
	  fi; \
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
	      if [ "$$OS" = "linux" ]; then \
	        for distro in ubuntu arch; do \
	          cp "$$D/$$PKG.tar.gz" "$$D/$$BIN-$$V-$$distro-$$ARCH.tar.gz"; \
	          chmod 0644 "$$D/$$BIN-$$V-$$distro-$$ARCH.tar.gz" || true; \
	          echo "Wrote $$D/$$BIN-$$V-$$distro-$$ARCH.tar.gz"; \
	        done; \
	      fi; \
	      rm -rf "$$STAGE"; \
	      PACKED=1; \
	    done; \
	  done; \
	fi; \
	if [ "$$PACKED" -eq 0 ]; then \
	  echo "No built binaries found to package. Searched TARGETS and target/*/release."; \
	fi
