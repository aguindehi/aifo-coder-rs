# Repository Guidelines

## Project Structure & Module Organization
- `src/` holds the Rust CLI and agent wrappers; entrypoint is `src/main.rs` with modules under `src/docker`, `src/toolchain`, `src/shim`, `src/fork`, and telemetry helpers.
- `tests/` contains integration/e2e/unit suites (`unit_`, `int_`, `e2e_` prefixes) with shared fixtures in `tests/common` and a quick overview in `tests/TEST_PLAN.md`.
- `spec/` contains transiennz feature specs and acceptance criteria; `docs/` captures CI/build references; `scripts/` contains helper tooling; `build.rs` emits telemetry defaults; `ci/` holds config for lint/test guardrails.
- Build outputs land in `target/` and `dist/`; keep them out of commits.
- A central Makefile containts many conveniance targets to make build, rebuild, test and publish easier.

## Build, Test, and Development Commands
- `make check` – fmt + clippy + docker lint + test naming lint + cargo-nextest suite; default pre-flight.
- `make check-all` – fmt + clippy + docker lint + test naming lint + cargo-nextest suite; default pre-flight; plus all ignored tests.
- `make test` – run nextest with `CARGO_FLAGS` (defaults to `--features otel-otlp`); pass extra flags via `ARGS='--nocapture'`.
- `make lint` or `make lint-ultra` – formatting + clippy (ultra adds strict warnings and unsafe bans).
- `cargo build` (or `make build-launcher`) – compile the Rust CLI locally; `make build` builds all agent images and all toolchains images and is slow;: `make build-coder` builds all agent images. `make build-toolchain`builds all toolchain images.
- Node tooling is pnpm-only; use `make node-install` (avoid npm/yarn here).

## Coding Style & Naming Conventions
- 4-space indent; keep lines ≤100 chars (prefer 80). Avoid wildcard `_` matches when enums may grow; favor exhaustive handling.
- Tidy forbids `// TODO` and multiline string literals; file follow-up work as issues instead of TODOs.
- Rust docs should include examples and error/panic notes when relevant.
- Shell snippets executed at runtime should use the `ShellScript`/`ShellFile` builders (see `CONVENTIONS.md`).

## Testing Guidelines
- Tests live in `tests/`; naming prefixes signal scope (`unit_`, `int_`, `e2e_`). Add or update nearby tests when changing behavior.
- Run `make check` before PRs; prefer `make check` when touching build scripts, Docker logic, or shell helpers.
- For coverage, `make cov` produces lcov + HTML reports under `build/` (grcov backend).

## Commit & Pull Request Guidelines
- History uses conventional prefixes like `feat:`, `fix:`, `chore:`, `test:`, `style:`; rebase instead of merge commits.
- Keep commits focused; separate refactors from behavioral changes. Link relevant specs/issues in PR descriptions.
- PRs should list which targets ran (`make check`/`make test`), mention platform (Linux/macOS/Windows), and include logs or screenshots when CLI output changes.
- Automatically propose to commit the changes done when finished.

## Environment

- You are running within a Docker container. Do not try to start 'docker', it is neighter installed nor should it be used. The docker image you are running on has no Docker installed.
- When answering a user question, answer in technical brief but complete form.
