# aifo-coder support

The support subcommand renders a colorized coder/toolchain support matrix.

- Fast and non-blocking: checks run back-to-back; animation never delays exploration.
- Randomized order: cells are shuffled to keep scattered updates lively.
- TTY-only animation: spinners on interactive terminals; non-TTY prints a static matrix.

Usage
- aifo-coder support

Environment controls
- AIFO_SUPPORT_AGENTS: CSV list of agents (default: aider,crush,codex,openhands,opencode,plandex)
- AIFO_SUPPORT_TOOLCHAINS: CSV kinds (default: rust,node,typescript,python,c-cpp,go)
- AIFO_SUPPORT_ANIMATE=0: disable animation (even if TTY)
- AIFO_SUPPORT_ASCII=1: ASCII spinner frames (-\|/)
- AIFO_SUPPORT_ANIMATE_RATE_MS: spinner cadence (default 80; clamp to [40, 250])
- AIFO_SUPPORT_RAND_SEED: u64 seed for deterministic shuffle (printed when verbose)
- AIFO_SUPPORT_NO_PULL=1: inspect images; mark FAIL when not present locally (no pull)

See also
- docs/support-matrix.md for a deeper dive, sample output and troubleshooting.
