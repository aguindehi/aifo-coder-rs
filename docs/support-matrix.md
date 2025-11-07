# aifo-coder support (coder/toolchain matrix)

Summary
- Fast, non-blocking exploration of all coder/toolchain pairs with randomized order.
- TTY-only animation on stderr; non-TTY produces a static matrix after checks complete.
- Worker runs checks back-to-back with zero artificial sleeps; painter animates on tick only.

Usage
- Run the command:
  - aifo-coder support
- On interactive terminals (TTY), a single spinner cell animates while results land scattered.
- On non-TTY, a static matrix is printed after checks finish.

Environment controls (AIFO_SUPPORT_*)
- AIFO_SUPPORT_AGENTS: CSV override of agents (default: aider,crush,codex,openhands,opencode,plandex)
- AIFO_SUPPORT_TOOLCHAINS: CSV override of toolchains (default: rust,node,typescript,python,c-cpp,go)
- AIFO_SUPPORT_NO_PULL=1: inspect image first; mark FAIL if image is not present locally.
- AIFO_SUPPORT_TIMEOUT_SECS: soft per-check timeout (default: none); commands are expected quick.
- AIFO_SUPPORT_ANIMATE=0: disable animation (even if TTY).
- AIFO_SUPPORT_ASCII=1: use ASCII spinner frames (-\|/) instead of Unicode.
- AIFO_SUPPORT_ANIMATE_RATE_MS: spinner tick interval (default 80; clamped to [40, 250]).
- AIFO_SUPPORT_RAND_SEED: u64 seed for deterministic shuffle; printed when verbose.

Images and runtime
- Agent images: derived via src/agent_images::default_image_for_quiet(agent).
- Toolchain images: aifo_coder::default_toolchain_image(kind), including normalization.
- Docker runtime is required; on missing docker, prints a prominent red line and exits nonzero.

Status tokens
- PASS: agent OK and toolchain PM OK.
- WARN: exactly one OK.
- FAIL: neither OK or image/runtime error (e.g., not-present under NO_PULL).
- PENDING: spinner while checks are in-flight (TTY-only).

Terminal layout
- Agent column ~16 chars; cell columns ~6 chars (token + padding).
- When terminal width is tight, columns compress to single-letter colored tokens (G/Y/R).

Sample output (non-TTY, colors disabled)
  support matrix:
                 rust  node   ts    py    c-cpp  go
  aider          PASS  WARN   FAIL  PASS  WARN   PASS
  crush          WARN  PASS   WARN  PASS  FAIL   PASS
  codex          FAIL  WARN   WARN  PASS  PASS   FAIL
  openhands      PASS  PASS   WARN  WARN  PASS   PASS
  opencode       PASS  WARN   PASS  PASS  WARN   PASS
  plandex        WARN  PASS   FAIL  PASS  PASS   WARN

Notes
- Typescript maps to Node PM; uses "npx tsc --version || true" for presence detection.
- Python PM prefers "python3 --version".
- c-cpp PM attempts "gcc --version || cc --version || make --version".

Troubleshooting
- Set AIFO_SUPPORT_ANIMATE=0 to disable animation for CI logs.
- Use AIFO_SUPPORT_RAND_SEED for reproducible order in tests or comparisons.
- With AIFO_SUPPORT_NO_PULL=1, ensure images exist locally or expect FAIL tokens.
