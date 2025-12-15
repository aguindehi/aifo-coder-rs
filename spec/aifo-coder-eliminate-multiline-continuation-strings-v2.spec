# ignore-tidy-linelength
#
# Spec v2: eliminate multi-line string literals and source-level continuation strings everywhere.
#
# Status: v2 (revised + consistency pass)

## 0. Executive summary

We enforce a repository-wide Rust source rule:

- No Rust string literal may span multiple *source* lines anywhere in this repository.
- No Rust “line continuation string” may be used (e.g. `"\` at end-of-line).
- Multi-line runtime strings are allowed only if constructed by joining validated single-line fragments.

This is stricter than the existing “ShellScript for `sh -c`” convention: it applies to *all* strings,
including doc attributes, test fixtures, file templates, and protocol text.

This v2 spec revises v1 to:
- Align new helpers with existing builder conventions (`ShellScript`, `ShellFile`).
- Separate “control scripts” (single-line passed to `sh -c`) from “file content” (multi-line written).
- Define a consistent validation invariant (reject `\n`, `\r`, `\0`) across all builders.
- Clarify naming, module placement, error-handling behavior (debug vs release), and adoption steps.
- Identify gaps in v1 and correct them (docs migration strategy, boundary enforcement, CI guard shape).

## 1. Definitions and scope

### 1.1 Forbidden patterns (source-level)

A “multi-line literal” is any Rust string literal (normal or raw) that contains a newline between the
opening and closing delimiter *in the source file*.

Forbidden examples:

- Multi-line raw string:
  `let s = r#"line1
line2"#;`

- Multi-line normal string:
  `let s = "line1
line2";`

- Backslash-continued string literal:
  `let s = "\
line1\r\n\
line2\r\n";`

Forbidden also includes literals embedded in macros (`format!`, `println!`, `concat!`, `include_str!`,
etc.) if the literal itself spans multiple lines.

### 1.2 Allowed patterns (source-level)

Allowed patterns are those where *every literal is single-line in the source*:

- Joining fragments (simple cases):
  `let s = ["line1", "line2"].join("\n");`

- Builder-based construction (preferred for correctness):
  - shell control scripts executed via `sh -c`: `ShellScript`
  - script files written to disk: `ShellFile`
  - general multi-line text files/fixtures: `TextLines` (new)

### 1.3 Scope

This rule applies to:
- `src/**`, `tests/**`, `build.rs`
- All doc attributes and module docs
- Tests and fixture/template content
- Shell-executed strings, file templates, and protocol strings

Non-goal: removing runtime newlines entirely. Multi-line runtime strings are necessary; the
requirement is that they must not come from multi-line literals in *source*.

## 2. Required invariants (correctness + security)

### 2.1 “No embedded newline or NUL” in fragments

All “line fragments” (builder inputs) must reject:
- `\n` and `\r` (prevents accidental multi-line injection and keeps determinism)
- `\0` (avoid weird shell/file/path boundary behavior)

This invariant is already present conceptually (see `reject_newlines` helper). Builders must enforce
it consistently.

### 2.2 Shell safety and determinism

For any string executed by a shell interpreter via a `-c` style option:
- MUST be constructed from atomic fragments and joined by a builder (`ShellScript`).
- MUST NOT inline untrusted/user-provided data into the control script text.
  Instead, pass argv as positional params (`"$@"`) or validated environment variables.

### 2.3 Preview must match execution

If a code path has both:
- a “preview” rendering of a command or script, and
- a “real” execution of that command or script,

then both must share the same construction logic (same builder + same fragments).
This prevents subtle divergences and makes the “no embedded newlines” invariant enforceable.

## 3. Builders and naming (consistency with existing code)

We already have two builders with a shared shape:

- `ShellScript` (single-line control scripts):
  - `new() -> Self`
  - `push(fragment) -> &mut Self`
  - `extend(iter) -> &mut Self`
  - `build() -> io::Result<String>`
  - Validates fragments contain no `\n`, `\r`, `\0`
  - Joins fragments with `"; "` into one line

- `ShellFile` (multi-line scripts written to disk):
  - `new() -> Self`
  - `push(line) -> &mut Self`
  - `extend(iter) -> &mut Self`
  - `build() -> io::Result<String>` (joins lines with `\n` and adds trailing `\n`)
  - Validates each line contains no `\n`, `\r`, `\0`

### 3.1 New builder: TextLines (multi-line text content)

We introduce `TextLines` to generate multi-line runtime content from validated single-line inputs.

It MUST follow the same “builder shape” convention as ShellScript/ShellFile:

- `TextLines::new() -> Self`
- `push(line: impl Into<String>) -> &mut Self`
- `extend(iter: impl IntoIterator<Item = impl Into<String>>) -> &mut Self`
- `build_lf() -> io::Result<String>`: join with `\n`, add trailing newline
- `build_crlf() -> io::Result<String>`: join with `\r\n`, add trailing CRLF

Validation:
- Each pushed line must be validated to reject `\n`, `\r`, `\0`.

Debug vs release behavior (consistent with v1 intent, but clarified):
- In debug builds, use `debug_assert!(!contains_forbidden_chars(line))`.
- In release builds, return `io::ErrorKind::InvalidInput` with stable message.

Rationale:
- ShellFile is specifically for scripts written to disk; TextLines is the general-purpose equivalent
  for fixtures/templates/config/protocol blocks written to files or buffers.

Naming note:
- We use `TextLines` (not `TextFile`) because it mirrors `ShellScript`/`ShellFile`:
  - ShellScript = control script builder (single line)
  - ShellFile   = script file builder (multi line, LF, trailing newline)
  - TextLines   = general text-file-like builder (multi line, LF/CRLF explicit)

We can later add a `TextFile` type if we need file-specific semantics (shebang constraints, encoding,
or permissions). v2 does not require it; we keep the surface area minimal.

### 3.2 Optional helper: HttpHeaderLines (CRLF-critical protocol text)

For protocol strings where CRLF is semantically required (HTTP):
- Prefer a dedicated helper that:
  - validates each header line (no CR/LF/NUL)
  - joins with `\r\n`
  - appends terminator `\r\n\r\n` in a controlled way

This is optional but recommended to avoid ad-hoc `"\\r\\n"` concatenation and to keep tests readable.

## 4. Repository guardrails (prevent regressions)

### 4.1 Source scan checks

We add (or extend) a check that fails if either of these find matches:
- multi-line raw string starts (coarse but effective):
  - `rg -n 'r#"' src tests build.rs`
- line-continuation strings:
  - `rg -n '"\\$' src tests build.rs`

Important: the `r#"` grep is intentionally conservative; it may also match single-line raw strings.
To avoid false positives we should refine the check to detect actual multi-line raw strings. Options:

Option A (simple, acceptable):
- Keep the coarse grep, but allow single-line raw strings by using a more precise pattern that
  detects newline between delimiters (hard in ripgrep alone).

Option B (preferred for correctness):
- Implement a small Rust “tidy” checker:
  - parse Rust files as text
  - detect string/raw-string tokens spanning multiple source lines
  - detect `"\` continuation patterns
  - report exact file+line+snippet
This avoids false positives and will be stable over time.

v2 recommendation:
- Phase the guard:
  - Start with grep in early phases for fast feedback.
  - Replace with a small checker once the bulk cleanup is complete.

### 4.2 Boundary validation for shell `-c` usage

Any API that accepts a `script: &str` intended for `sh -c` MUST call the existing helper
`reject_newlines(script, "…")` (or equivalent) before executing.

Additionally, tests helper `docker_exec_sh` should validate the script is single-line (reject CR/LF)
to catch regressions early.

## 5. Verified gaps/issues in v1 and corrections in v2

1) **Builder naming / pattern mismatch**
   - v1 introduces `TextLines` but does not ensure it matches the existing builder shape.
   - v2 defines a consistent builder API mirroring ShellFile/ShellScript.

2) **ShellFile vs TextLines unclear boundary**
   - v1 suggests TextLines for “file content”, but we already have ShellFile for scripts.
   - v2 clarifies:
     - ShellFile = script files written to disk (shell-oriented)
     - TextLines = general multi-line text blobs (fixtures, configs, templates, toml, rust source)

3) **Doc attributes strategy under-specified**
   - v1 says “doc comments preferred” but does not give mechanical guidance.
   - v2 codifies:
     - convert multi-line `#![doc = r#"...` to `//!` comments where possible
     - ensure no new multi-line literal is introduced as replacement

4) **CI guard ambiguity**
   - v1 relies on `rg 'r#"'`, which is too broad (matches single-line raw strings).
   - v2 proposes phased guardrails with a preferred Rust checker for correctness.

5) **Line endings**
   - v1 wants LF/CRLF but doesn’t specify where to use which.
   - v2 specifies:
     - ShellFile uses LF (as today) unless Windows-specific requirements exist
     - TextLines supports LF and CRLF; choose based on protocol/file format needs
     - HTTP uses CRLF helper (protocol requirement)

## 6. Phased implementation plan (updated)

### Phase 0: Policy + builder groundwork (small, mechanical)
Goals:
- Introduce `TextLines` builder with the consistent API described above.
- Add unit tests verifying:
  - rejects CR/LF/NUL in each pushed line
  - correct `build_lf()` output with trailing newline
  - correct `build_crlf()` output with trailing CRLF
- Add boundary validation for shell `-c` helpers (tests and runtime entrypoints).

Acceptance:
- `TextLines` exists, is used at least once (no dead code).
- `make check` passes.

### Phase 1: Fix core runtime templates (highest user impact)
Targets (from v1 inventory):
- Replace multi-line raw string templates in core runtime modules:
  - script templates written to disk:
    - MUST use `ShellFile` (not TextLines), to match existing conventions.
  - general text templates/fixtures:
    - use `TextLines`.

Acceptance:
- No multi-line literals remain in these core modules.
- Script contents remain byte-equivalent (or explicitly documented if line ending normalization occurs).

### Phase 2: Fix protocol/test continuation strings
Targets:
- Replace any `"\` continuation usage in tests and runtime code.
- For HTTP request/response fixtures:
  - use join-from-lines or the `HttpHeaderLines` helper.
  - preserve exact CRLF semantics and header terminator.

Acceptance:
- `rg -n '"\\$' src tests build.rs` returns no matches.

### Phase 3: Fix doc attributes (source-wide cleanliness)
Targets:
- Convert multi-line `#![doc = r#"...` to module docs using `//!`.
- Avoid introducing new multi-line string literals as replacement.
- Keep documentation content stable.

Acceptance:
- No multi-line doc literals remain.

### Phase 4: Fix remaining tests/fixtures/templates
Targets:
- Convert all remaining multi-line raw strings used for fixtures:
  - POSIX scripts written to disk: `ShellFile`
  - config/toml/rust fixture content: `TextLines`
  - any execution via `sh -c`: `ShellScript`

Acceptance:
- No remaining multi-line literals or continuation strings.

### Phase 5: Enforce and centralize invariants
Targets:
- Ensure every `sh -c`/`-lc` execution path uses `ShellScript` or strict validation.
- Ensure preview and execution share the same builder.
- Replace coarse grep guard with a precise checker if false positives are problematic.

Acceptance:
- Boundary APIs reject CR/LF/NUL in control scripts.
- One guard mechanism (grep or checker) is in CI/`make check`.

### Phase 6: Repository guard (final hardening)
Targets:
- Add a stable CI check step (or `make check` step) that enforces:
  - no multi-line literals in Rust source
  - no continuation strings
- Keep the rule documented in CONTRIBUTING/CONVENTIONS if appropriate.

Acceptance:
- Regression immediately fails CI.
- `make check` passes.

## 7. Acceptance criteria (final)

1) No Rust string literal spans multiple source lines anywhere in `src/**`, `tests/**`, `build.rs`.
2) No line continuation strings (`"\` at EOL) in the same scope.
3) All shell `-c`/`-lc` payloads are:
   - built using `ShellScript`, or
   - validated to contain no CR/LF/NUL at boundary APIs.
4) All multi-line file content is built from validated single-line fragments:
   - scripts written to disk: `ShellFile`
   - general text fixtures/templates: `TextLines`
5) Previews match execution (shared builder fragments).
6) `make check` passes.

## 8. Notes / guidance

- Prefer <= 100 char lines in source where practical (see CONVENTIONS.md).
- Avoid introducing dead code: new helper builders must be used as part of Phase 0–1.
- Avoid large `"\n"` escape-sequence blobs; for anything non-trivial use `TextLines`/`ShellFile`.
- Preserve user-visible messages and test golden outputs unless explicitly changed.
