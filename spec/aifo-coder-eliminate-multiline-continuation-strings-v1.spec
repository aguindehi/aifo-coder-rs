# ignore-tidy-linelength
#
# Spec: eliminate multi-line string literals and source-level continuation strings everywhere.
#
# Status: v1 (phased plan)

## 0. Executive summary

We will enforce a project-wide rule:

- No Rust string literal may span multiple *source* lines anywhere in this repository.
- No Rust “line continuation string” may be used (e.g. `"\` at EOL).
- Multi-line runtime strings are allowed only if they are constructed by joining validated
  single-line fragments.

This is stricter than the existing “ShellScript for /bin/sh -c” convention: it applies to all
strings, including doc attributes, test fixtures, file templates, and protocol strings.

We will implement this by:
1) Introducing a reusable `TextLines` builder for multi-line file content (LF/CRLF) built from
   validated single-line fragments.
2) Replacing each offending multi-line literal or continuation string with `TextLines` or `join()`.
3) Adding targeted debug-time and release-time guards at boundary APIs to prevent regressions.
4) Adding tests and a simple grep-based CI guard.

## 1. Definitions and scope

### 1.1 Forbidden patterns (source-level)

A “multi-line literal” is any string literal that in the Rust *source code* contains a newline
character between the opening and closing quotes or raw-string delimiters.

Forbidden examples:
- Multi-line raw string:
  `let s = r#"line1
line2"#;`

- Multi-line normal string:
  `let s = "line1
line2";`

- Backslash-continued string literal (C-style):
  `let s = "\
line1\r\n\
line2\r\n";`

- Multi-line raw doc attributes:
  `#![doc = r#"foo
bar"#]`

Forbidden also includes multi-line literals inside macros such as `format!`, `println!`, `concat!`,
etc, if the literal itself spans multiple lines.

### 1.2 Allowed patterns (source-level)

Allowed patterns are those where every literal is single-line in the Rust source:

- Joining fragments:
  `let s = ["line1", "line2"].join("\n");`

- Building protocol text using `push_str` or joining lines:
  `let header = ["HTTP/1.1 200 OK", "Content-Length: 0", "", ""].join("\r\n");`

- Single-line literals that include `"\n"` or `"\r\n"` escape sequences are permitted but strongly
  discouraged for anything non-trivial (see §3.4).

### 1.3 Scope

This rule applies to:
- `src/**`, `tests/**`, and `build.rs`
- All doc attributes and module docs
- Tests and fixture/template content
- Shell-executed strings, file templates, and protocol strings

Non-goal: removing runtime newlines entirely. Multi-line runtime strings remain necessary for
writing scripts and fixtures; they must be constructed from validated single-line fragments.

## 2. Design constraints and correctness requirements

### 2.1 “No newlines in fragments” as an invariant

Any builder that constructs a script or multi-line text must validate fragments:
- must reject `\n` and `\r` in *input fragments*
- must reject `\0` to prevent injection/pathological behavior

### 2.2 Shell safety and determinism

For any string that is executed by a shell interpreter via `sh -c`, `bash -c`, PowerShell, etc:
- MUST be constructed from atomic fragments and joined by a builder
- MUST not embed user-controlled data directly into the script string; use positional arguments
  (`"$@"`) or environment variables (`-e KEY=...`) for untrusted values.

### 2.3 Previews must match execution

If a code path has both:
- “preview” rendering of an internal shell command, and
- “real” execution of that shell command,

then both must share the same construction logic (same builder, same line fragments), to prevent
divergence.

## 3. Standard library utilities to use

### 3.1 ShellScript (existing)

Use `crate::ShellScript` for any string executed by `/bin/sh -c` (and similar shell `-c` forms),
because it enforces the invariant that each fragment has no embedded newlines.

- Each push/extend fragment must be single-line (no `\n`/`\r`)
- `build()` joins with `; ` to produce a single-line control script
- In debug: `debug_assert!`
- In release: return `io::Error`

### 3.2 TextLines (new)

Introduce a new helper builder for *file content* that must contain newlines (scripts written to
disk, fixtures, config snippets, etc).

Required API (final names can vary, but semantics must match):
- `TextLines::new()`
- `push(line: impl Into<String>) -> &mut Self`
- `extend(iter: impl IntoIterator<Item = String>) -> &mut Self`
- `build_lf() -> io::Result<String>` joins with `\n` and appends trailing `\n` optionally
- `build_crlf() -> io::Result<String>` joins with `\r\n` and appends trailing `\r\n` optionally

Validation:
- For every pushed line, reject `\n`, `\r`, `\0`
- (Optional) expose a strict mode requiring non-empty first line for shebang scripts

Rationale:
- It removes multi-line raw string literals while still producing multi-line runtime content.
- It makes line endings explicit and consistent across platforms.

### 3.3 HTTP header builder (recommended)

For protocol strings where CRLF is significant:
- Use a small helper that takes header lines and joins with `\r\n`, then adds the header terminator
  `\r\n\r\n` as a separate fragment.
- Ensure each header line is validated as single-line (no CR/LF/NULL).

This avoids reintroducing `"\` continuations in tests and keeps behavior correct.

### 3.4 Escaped newline sequences in single-line literals

Single-line literals that contain escape sequences like `"\n"` are:
- allowed for small trivial cases (<= 1–2 lines)
- discouraged for anything larger; use `TextLines`

Rule of thumb:
- If you would write a raw multi-line string, instead use `TextLines`.

## 4. Inventory and risk analysis (validated against repo grep output)

We must address two concrete classes of offenders found by:
- `rg -n 'r#"' src tests build.rs`
- `rg -n '"\\$' src tests build.rs`

### 4.1 Multi-line raw strings (`r#"`)

High-risk offenders (must be refactored):
- `src/toolchain/shim.rs` multi-line script templates: `shim`, `sh_wrap`
- `src/fork/inner.rs` tmux launch script template
- tests with multi-line script/file fixtures:
  - `tests/e2e_wrapper_behavior.rs` (POSIX script and Windows batch)
  - `tests/e2e_acceptance_rust_sidecar_smoke.rs` (Cargo.toml and lib.rs fixtures)
  - `tests/int_home_writability_agents.rs` (multi-line shell script)
  - `tests/e2e_config_copy_policy.rs` and other E2E tests with multi-line `script = r#"...`

Doc attributes:
- `src/support.rs` uses `#![doc = r#"...` (multi-line)
- `src/lib.rs` uses `#![doc = r#"...` (multi-line)

Medium/low risk: raw strings that are already single-line in source are not a problem
(e.g. many `r#"..."#` that do not contain newlines). They are allowed.

### 4.2 Backslash-continued strings (`"\` at EOL)

Confirmed occurrences in `src/toolchain/http.rs` tests:
- request bodies and headers built with `"\` continuation

These must be rewritten to join single-line fragments (Vec join or a dedicated HTTP builder).

### 4.3 Shell `-c`/`-lc` call sites

Any site calling `.arg("-c")` or `.arg("-lc")` must ensure the script payload is a single-line
string. If it is more complex than a trivial one-liner, use `ShellScript`.

Additionally, boundary helpers that accept `script: &str` for `sh -c` must enforce the invariant.

## 5. Gaps and corrections to the initial plan (resolved here)

### 5.1 Docstrings are not “scripts” but still violate “no multi-line literals”

The earlier plan mentioned doc attributes but did not mandate a specific replacement.
This spec mandates converting multi-line `#![doc = r#"...` to:
- multi-line doc comments (`//!` / `///`) OR
- join-based doc attributes using single-line fragments

Preferred: doc comments, because it avoids large string expressions and is idiomatic Rust.

### 5.2 Avoid “concat!(...)” as the primary solution

`concat!()` can still embed newline escape sequences and creates large monolithic expressions that
are hard to review. Prefer:
- doc comments for documentation
- `TextLines` for file templates
- `ShellScript` for control scripts

### 5.3 Preview-vs-run divergence in docker run preview

The earlier plan noted `build_docker_preview_only()` uses a large formatted script.
This spec requires:
- preview-only path must call the same builder used by execution (`build_container_sh_cmd()`),
  so both obey the same invariants and remain consistent.

### 5.4 Tests as a regression vector

Tests frequently embed fixture blobs as multi-line raw strings. This spec explicitly requires:
- converting these to join-based construction
- adding a small CI/Make check step to keep them from reappearing

## 6. Phased implementation plan

### Phase 0: Policy + tooling groundwork (small, mechanical)
- Add `TextLines` builder under `src/util/` and re-export in `src/util/mod.rs` (or consistent module).
- Add unit tests for `TextLines` (rejects CR/LF/NUL; correct LF/CRLF joining).
- Add debug-time and release-time guards for boundary APIs:
  - `tests/support/mod.rs::docker_exec_sh`: reject scripts containing `\n` or `\r`
    (debug_assert + return error tuple).
  - Any similar helpers that accept a script string.

Acceptance:
- `TextLines` exists and tests pass.
- No functional changes yet, only new helper and invariants.

### Phase 1: Fix core runtime templates (highest user impact)
- `src/toolchain/shim.rs`:
  - Replace multi-line raw string templates with `TextLines`.
  - Preserve exact script contents and line endings (LF by default).
- `src/fork/inner.rs`:
  - Replace tmux script template with `TextLines`.

Acceptance:
- `rg -n 'r#"'` no longer matches multi-line literals in these modules.
- Behavior unchanged (scripts written match previous byte-for-byte, aside from any intentional
  line-ending normalization which must be specified).

### Phase 2: Fix protocol/test continuation strings
- `src/toolchain/http.rs` tests:
  - Replace `"\`-continued request text with join-from-lines.
  - Ensure CRLF semantics remain correct (headers terminated by CRLFCRLF).
- Ensure any similar patterns in other tests are fixed.

Acceptance:
- `rg -n '"\\$' src tests build.rs` returns no matches.

### Phase 3: Fix doc attributes (source-wide cleanliness)
- Convert multi-line `#![doc = r#"...` in:
  - `src/lib.rs`
  - `src/support.rs`
  to `//!` module docs (preferred), preserving content.

Acceptance:
- `rg -n 'r#"' src tests build.rs` no longer reports those multi-line doc literals.

### Phase 4: Fix remaining test fixtures and scripts (broad sweep)
- Convert all remaining multi-line raw strings in tests:
  - scripts (POSIX + Windows)
  - Cargo.toml/lib.rs fixtures
  - E2E scripts
  to join-based construction using `TextLines`.

Acceptance:
- `rg -n 'r#"' src tests build.rs` returns no matches.

### Phase 5: Fix remaining shell `-c` sites and enforce consistency
- For each `.arg("-c")` / `.arg("-lc")` site:
  - ensure script payload is built via `ShellScript` (or is a trivial one-liner validated to have no CR/LF)
- Ensure docker preview-only uses the same builder path as docker exec.

Acceptance:
- All `-c` payloads are single-line and validated.
- Preview strings match execution strings for the same feature.

### Phase 6: Add repository guard (CI / local)
Add a check step (CI or Makefile) that fails if either command returns matches:
- `rg -n 'r#"' src tests build.rs`
- `rg -n '"\\$' src tests build.rs`

Acceptance:
- A new offender fails CI immediately.

## 7. Acceptance criteria (final)

The change is accepted when:
1) `rg -n 'r#"' src tests build.rs` produces no matches.
2) `rg -n '"\\$' src tests build.rs` produces no matches.
3) All shell `-c` / `-lc` scripts are single-line and are constructed via `ShellScript` or are
   explicitly validated one-liners.
4) All multi-line file templates/fixtures are constructed from single-line fragments using
   `TextLines` (or equivalent join-based construction with fragment validation).
5) `make check` passes.

## 8. Notes

- Do not change user-visible strings unless required; preserve script/fixture behavior.
- Avoid introducing dead code; all new helpers must be used by at least one refactor in Phase 0–1.
- Prefer lines <= 100 chars where feasible; add ignore-tidy-linelength only when necessary.
