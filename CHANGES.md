2025-11-07 AIFO User <aifo@example.com>

Add v3 support: fast randomized support matrix

- Add "support" CLI subcommand and module scaffolding.
- Implement randomized worklist and worker/painter split (TTY-only animation).
- Cache agent --version checks; repaint rows immediately on cell completion.
- Add docs and tests: docker missing, deterministic shuffle, agent check count.

2025-09-29 AIFO User <aifo@example.com>

Add v4 spec: real installs for openhands/opencode/plandex

- Add spec/aifo-coder-implement-openhands-opencode-plandex-v4.spec with comprehensive plan.
- Detail OpenHands (uv tool install), OpenCode (npm global), Plandex (Go build) recipes.
- Document CA handling, cleanup patterns, multi-arch, and reproducibility.
- Outline Makefile targets, Dockerfile stage changes, tests (preview-only), and docs updates.
