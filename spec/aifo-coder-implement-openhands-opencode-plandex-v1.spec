Title Implement three additional CLI coding agents: OpenHands, OpenCode, Plandex.

Goal Add first-class support for launching three new agent containers, analogous to Aider, Crush and Codex:

 • New CLI subcommands: openhands, opencode, plandex
 • Container run path, environment and mounts identical in spirit to existing agents
 • Images listed by aifo-coder images and selectable via the same flavor/registry rules
 • Fully compatible with toolchain sidecars, proxy and shims

Out of scope

 • Building, publishing and securing the agent images themselves (we define requirements)
 • Agent-specific UX or configuration beyond what existing agents share
 • Toolchain-sidecar changes; none needed for routing/shims

Image and runtime assumptions

 • Image naming follows default_image_for(agent): <AIFO_CODER_IMAGE_PREFIX>-{-slim}:<AIFO_CODER_IMAGE_TAG> e.g.
   repository.migros.net/aifo-coder-openhands:latest
 • Each image must provide the agent executable in PATH and include:
    • curl (for shims fallback and diagnostics)
    • POSIX shell (/bin/sh) and core tools
    • /opt/aifo/bin present in PATH; it must be writable at build time and readable at runtime
    • No privileged mode, no host docker socket
 • We will re-use environment passthroughs (AIFO_API_*, TZ, EDITOR, GNUPGHOME, etc.) as-is

Phased plan

Phase 0 — Architecture decisions and image prep (planning)

 • Decide agent binary names inside containers:
    • openhands, opencode, plandex in /usr/local/bin or equivalent
 • Decide initial PATH strategy:
    • Start with "default" mapping that places /opt/aifo/bin early, identical to Aider/default
    • If a given agent is Node-based and requires node before shims (like Codex/Crush), we can switch it to the Codex/Crush PATH template in a follow-up
      (guarded by a small constant)
 • Define minimal image requirements (above) and produce both full and -slim variants
 • Ensure images honor HOME=/home/coder and run fine with user mapping and /workspace mounts

 Phase 1 — CLI: add three subcommands and wire agent selection

 • Add clap subcommands in src/cli.rs:
    • Agent::OpenHands { args: Vec }
    • Agent::OpenCode { args: Vec }
    • Agent::Plandex { args: Vec }
 • Update CLI help/usage strings with brief one-line descriptions
 • Update resolve_agent_and_args (src/main.rs) to map:
    • OpenHands -> "openhands"
    • OpenCode  -> "opencode"
    • Plandex   -> "plandex"
 • Update maybe_warn_missing_toolchain_agent (src/warnings.rs) to include the new agent names so users receive the same “no toolchain sidecars” guidance when
   appropriate

Phase 2 — Images command output

 • Update aifo-coder images (src/commands/mod.rs):
    • Print effective image refs for openhands, opencode, plandex
    • Emit the same machine-readable stdout lines: openhands , opencode , plandex
 • No changes needed to agent_images.rs:
    • default_image_for() is already generic and composes -{-slim}:
    • Registry prefix logic remains shared

Phase 3 — Docker run wiring and PATH policy

 • In build_docker_cmd (src/docker.rs) ensure agent "openhands"/"opencode"/"plandex" receive a sane PATH. Initial plan:
    • Use the default PATH branch (shims early) for all three
    • If later an agent proves Node-based and needs native node before shims, add it to the codex/crush branch while keeping the default as-is
 • All other envs/mounts remain identical (HOME/GNUPGHOME, gitconfig, .aider/.codex/.crush, logs mount, GPG setup, XDG_RUNTIME_DIR, optional AppArmor,
   session network, etc.)
 • Container naming remains: aifo-coder---
 • No changes needed to locks, proxy, shims or sidecars for agent runs

Phase 4 — Fork flows (no functional change needed)

 • Fork orchestration already treats agent opaquely via "agent" string
 • Ensure fork warnings functions include the new agent names, so guidance and prompts appear
 • No changes to orchestrators, session metadata or post-merge flows required

Phase 5 — Tests

 • Unit tests
    • src/cli.rs: parsing of subcommands (minimal smoke) if warranted
    • src/commands/mod.rs: images command should include three new lines on stdout
 • Preview-only tests (no docker pull):
    • Re-use build_docker_cmd() to assert:
       • Preview commands include --name/--hostname using the agent string
       • PATH contains /opt/aifo/bin (and optionally ensure user mapping is included)
 • Do not add tests that pull or run the new images by default; keep CI fast/offline
 • Optional: extend an existing “images” test scaffold to assert new lines exist

Phase 6 — Documentation and help text

 • Extend CLI help in README or usage output (if maintained) to list the three agents
 • Document image naming, flavor and registry overrides are identical to existing agents

Phase 7 — Rollout controls and compatibility

 • Backward-compatible: no change for existing agents or toolchain flows
 • Registry prefix and flavor controls continue to work globally via env
 • Allow users to override images per run using --image as they can today

Change checklist (brief diffs, by file)

 • src/cli.rs
    • Add Agent variants: OpenHands, OpenCode, Plandex
    • Subcommand docs; maintain trailing_var_arg behavior
 • src/main.rs
    • resolve_agent_and_args: map new Agent variants to their agent string
    • maybe_warn_missing_toolchain_agent: include new names in agent filter (or refactor to a set of “coding agent” names to avoid future edits)
 • src/commands/mod.rs
    • run_images: print flavor/registry unchanged; add three image lines (stderr pretty + stdout)
 • src/warnings.rs
    • maybe_warn_missing_toolchain_agent: extend the allowed agent list to include new names
 • src/docker.rs
    • PATH policy: initially keep default branch for new agents
    • Optionally add comments indicating where to switch them to the codex/crush path if needed
 • agent images (external to repo build)
    • Build/publish: aifo-coder-openhands, aifo-coder-opencode, aifo-coder-plandex
    • Provide -slim variants and ensure curl + /opt/aifo/bin exist in all

Acceptance criteria

 • aifo-coder openhands -- --help (dry-run) prints a docker preview using correct image ref
 • aifo-coder opencode -- --help (dry-run) prints a docker preview using correct image ref
 • aifo-coder plandex -- --help (dry-run) prints a docker preview using correct image ref
 • aifo-coder images prints six agent lines (codex, crush, aider, openhands, opencode, plandex)
 • Warnings about missing toolchain sidecars appear for the three new agents when no sidecars
 • No regressions in existing tests; no network pulls during tests

Risk and mitigations

 • PATH ordering mismatch for a new agent: start with the safe default; adjust per agent with a small conditional branch (like codex/crush) after validating
   image behavior
 • Image availability: keep tests dry-run only; add machine-readable images output to aid ops
 • Windows/macOS nuances: re-use existing docker run flags and mounts; avoid platform-specific code

Telemetry/logging

 • No changes needed; previews and stderr info lines already include the agent string
 • aifo-shim/proxy flows are agent-agnostic

Follow-ups (optional)

 • Per-agent PATH policy env override (e.g., AIFO_AGENT_PATH_MODE) to switch between “shims-first” and “node-first” if needed without code changes
 • Add minimal smoke tests for “docker exists” environments if we later host prebuilt images

Estimated effort

 • Code changes: small (~150–250 LoC net), isolated to CLI wiring, warnings, images output and optional PATH switch
 • Tests: small additions mirroring existing patterns
 • Images: external; ensure they meet runtime assumptions

Rollout plan

 • Land code with tests (images output and preview tests do not require the new images present)
 • Build and publish images independently
 • Announce availability; users with the images present can run agents immediately
 • Iterate on PATH policy per agent as needed based on feedback

End of spec.
