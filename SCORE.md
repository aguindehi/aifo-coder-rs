## 1. Project Identification
- Project: AI Foundation Coder (Rust CLI + containerized agent/toolchain images)
- Repository purpose: secure, reproducible launcher for multiple coding agents with toolchain sidecars.

## 2. Context Summary
- CLI builds docker run invocations with constrained mounts/env, optional AppArmor, shim-first PATH, and per-session networks. Toolchain sidecars expose a proxy/shim for routing dev tools; fork mode clones workspaces for multi-pane experiments. (README.md, src/main.rs, src/docker/run.rs, src/toolchain_session.rs, src/bin/aifo-shim.rs)

## 3. Assumptions
- Read-only sandbox; did not execute make/test/nextest or Docker-based flows here.
- Assessment based on static code/doc review (src/, tests/, docs/README-testing.md, docs/README-security-architecture.md).

## 4. Findings Summary (counts per category)
- Architecture: 1
- Error Handling: 1
- Operational Robustness: 2
- Other categories: 0

Findings (IDs → severity → brief):

- F1 (High, Operational Robustness): Proxy binds 0.0.0.0 on Linux by default, widening exec surface. (src/toolchain/proxy.rs:697-735)
- F2 (Medium, Operational Robustness): Proxy spawns unbounded per-connection threads with default infinite read timeouts; slowloris DoS risk. (src/toolchain/proxy.rs:520-620, 697-770)
- F3 (Medium, Architecture): --docker-network-isolate sets a session network for agent runs but never creates it when toolchains are disabled, leading to docker run failures/inconsistent behavior. (src/main.rs:84-130, src/docker/run.rs:1155-1182)
- F4 (Medium, Error Handling): Chunked HTTP parsing reads declared chunk sizes into memory without bounding per-chunk size, enabling large allocations despite a 1 MiB body cap. (src/toolchain/http.rs:64-189)

## 5. Scorecard (with grades)
- Architecture & Design Quality: **72/100 (C)** — clear module boundaries, but network isolation gap and proxy exposure reduce design safety.
- Code Quality & Maintainability: **78/100 (C)** — organized Rust crate with helpers/builders; heavy env/mount code is complex but readable.
- Testing & Change Safety: **86/100 (B)** — extensive unit/int/e2e suites and documented lanes; gaps around new proxy/network edge cases noted above.
- Development Ergonomics: **80/100 (B)** — strong CLI surface, previews, pnpm migration helper; some env mutation (XDG) and network-isolate behavior surprises users.
- Reliability & Operability: **65/100 (D)** — proxy binding/timeout/connection-cap issues and missing network creation for isolated runs threaten uptime.
- Documentation Quality: **90/100 (A)** — rich README, security architecture, testing guides, and specs.

## 6. Overall Score
- **76/100 (C)** — solid baseline with strong docs/tests; reliability/security liabilities in proxy binding, connection limits, and network isolation reduce the grade.

## 7. Gap Summary
- Proxy exposure and lack of connection limits/timeouts conflict with “containment by default.”
- Network isolation flag is not self-contained for agent-only runs, causing failures and user surprise.
- Chunked request parsing lacks per-chunk cap, leaving a memory DoS vector.

## 8. Improvement Plan Summary
- R1 (Immediate): Default proxy to loopback/UDS, add bind-host flag, and enforce sane accept/timeout policies.
- R2 (Short): Cap chunk sizes before buffering and reject oversized chunked bodies.
- R3 (Short): Create/clean session network when --docker-network-isolate is used without toolchains (or block the flag in that mode) and add previews/tests.

## 9. Score Delta Budget
- R1 → addresses F1, F2. Deltas: +8 Architecture & Design, +12 Reliability & Operability, +4 Testing & Change Safety. Confidence: medium. Rationale: removes remote exposure, adds timeout/backpressure, and codifies behavior in tests.
- R2 → addresses F4. Deltas: +2 Architecture & Design, +6 Reliability & Operability, +4 Testing & Change Safety. Confidence: medium. Rationale: enforces parser bounds and adds regression coverage.
- R3 → addresses F3. Deltas: +3 Architecture & Design, +5 Reliability & Operability, +2 Development Ergonomics. Confidence: medium. Rationale: makes network isolation atomic and user-friendly.

(>50% of total delta comes from architectural/systemic reliability work: R1+R2+R3 architecture/reliability contributions dominate.)

## 10. Longitudinal History
- None yet; first scorecard in this repository snapshot.

## 11. Limitations
- No runtime validation or docker-backed tests executed in this read-only session; results are static-analysis-based.

## 12. Machine-Readable YAML
```
machine_readable:
  project: aifo-coder
  overall_score: 76
  scores:
    architecture_design_quality: {score: 72, grade: "C"}
    code_quality_maintainability: {score: 78, grade: "C"}
    testing_change_safety: {score: 86, grade: "B"}
    development_ergonomics: {score: 80, grade: "B"}
    reliability_operability: {score: 65, grade: "D"}
    documentation_quality: {score: 90, grade: "A"}
  findings:
    - {id: F1, category: Operational Robustness, severity: High, evidence: "src/toolchain/proxy.rs:697-735"}
    - {id: F2, category: Operational Robustness, severity: Medium, evidence: "src/toolchain/proxy.rs:520-620,697-770"}
    - {id: F3, category: Architecture, severity: Medium, evidence: "src/main.rs:84-130; src/docker/run.rs:1155-1182"}
    - {id: F4, category: Error Handling, severity: Medium, evidence: "src/toolchain/http.rs:64-189"}
  roadmap:
    - id: R1
      targets: [F1, F2]
      deltas: {architecture_design_quality: 8, reliability_operability: 12, testing_change_safety: 4}
      confidence: medium
    - id: R2
      targets: [F4]
      deltas: {architecture_design_quality: 2, reliability_operability: 6, testing_change_safety: 4}
      confidence: medium
    - id: R3
      targets: [F3]
      deltas: {architecture_design_quality: 3, reliability_operability: 5, development_ergonomics: 2}
      confidence: medium
```