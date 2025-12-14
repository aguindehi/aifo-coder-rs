## Line length

Lines should be at most 100 characters. It's even better if you can
keep things to 80.

**Ignoring the line length limit.** Sometimes – in particular for
tests – it can be necessary to exempt yourself from this limit. In
that case, you can add a comment towards the top of the file like so:

```rust
// ignore-tidy-linelength
```

## Tabs vs spaces

Prefer 4-space indent.

# Coding for correctness

Beyond formatting, there are a few other tips that are worth
following.

## Prefer exhaustive matches

Using `_` in a match is convenient, but it means that when new
variants are added to the enum, they may not get handled correctly.
Ask yourself: if a new variant were added to this enum, what's the
chance that it would want to use the `_` code, versus having some
other treatment?  Unless the answer is "low", then prefer an
exhaustive match. (The same advice applies to `if let` and `while
let`, which are effectively tests for a single variant.)

## Use "TODO" comments for things you don't want to forget

As a useful tool to yourself, you can insert a `// TODO` comment
for something that you want to get back to before you land your PR:

```
fn do_something() {
    if something_else {
            unimplemented!(); // TODO write this
    }
}
```

The tidy script will report an error for a `// TODO` comment, so this
code would not be able to land until the TODO is fixed (or removed).

This can also be useful in a PR as a way to signal from one commit that you are
leaving a bug that a later commit will fix:

```
if foo {
    return true; // TODO wrong, but will be fixed in a later commit
}
```

# How to structure your PR

How you prepare the commits in your PR can make a big difference for the
reviewer.  Here are some tips.

**Isolate "pure refactorings" into their own commit.** For example, if
you rename a method, then put that rename into its own commit, along
with the renames of all the uses.

**More commits is usually better.** If you are doing a large change,
it's almost always better to break it up into smaller steps that can
be independently understood. The one thing to be aware of is that if
you introduce some code following one strategy, then change it
dramatically (versus adding to it) in a later commit, that
'back-and-forth' can be confusing.

**Format liberally.** While only the final commit of a PR must be correctly
formatted, it is both easier to review and less noisy to format each commit
individually using `./x fmt`.

**No merges.** We do not allow merge commits into our history, other
than those by bors. If you get a merge conflict, rebase instead via a
command like `git rebase -i rust-lang/master` (presuming you use the
name `rust-lang` for your remote).

**Individual commits do not have to build (but it's nice).** We do not
require that every intermediate commit successfully builds – we only
expect to be able to bisect at a PR level. However, if you *can* make
individual commits build, that is always helpful.

# Naming conventions

Apart from normal Rust style/naming conventions, there are also some specific
to the compiler.

- Because `crate` is a keyword, if you need a variable to represent something
  crate-related, often the spelling is changed to `krate`.

# Dead code

As a general rule, do not create dead code or mark functions as dead code. If you
really need to do that, get user consent for it.

# Testing Rust Code

As a general rule use 'make check' instead of 'cargo test'. Make
check uses 'cargo nextest' which is a lot faster then 'cargo test'.

# Comments

As a general rule, do not use specification / plan phase informations in comments as
these are transient and specification / plan dependant.

# Recommended project-wide standard

This section is written for both humans and LLM/Coding Agents. Follow these
rules to keep shell behavior correct, portable and safe.

1) Never build executable shell scripts from multi-line string literals

Rule: any string that will be executed by /bin/sh -c, bash -c, cmd /c, PowerShell,
etc. must be assembled from atomic fragments and joined.

This prevents Rust source formatting from changing runtime behavior.

2) When to use which builder

- Use ShellScript (src/util/shell_script.rs) when the string will be executed via
  sh -c (e.g., docker exec … sh -c, sh -lc).
- Use ShellFile (src/util/shell_file.rs) when writing a script file that will be
  executed later (e.g., launch.sh, wrappers, entrypoints).

3) ShellScript builder (single-line control scripts)

Use the reusable helper “ShellScript” like:

 • ShellScript::new()
 • push(cmd: impl Into<String>)
 • extend([...])
 • build() → returns a single-line string joined with “; ”
 • validates: rejects \n, \r and \0 in every fragment

LLM/Coding-Agent rules:

- Each push()/fragment is one logical command with no embedded newlines.
- The builder joins fragments with “; ”, so never split compound constructs across
  fragments. Keep each of these in a SINGLE fragment:
  • if/then/elif/fi
  • for/do/done
  • case/esac
  • { … } or ( … )
- Prefer single-line constructs for compounds, for example:
  if cond; then do_x; elif other; then do_y; else do_z; fi
- Do not interpolate user-provided argv into the ShellScript text. Instead pass
  argv as positional parameters and use: exec "$@" inside the script.

4) ShellFile builder (multi-line scripts written to disk)

Use “ShellFile” when you intend to write a script file:

 • ShellFile::new()
 • push(line: impl Into<String>) → one logical line (no embedded \n/\r/\0)
 • extend([...])
 • build() → joins with “\n” and ensures a trailing newline

Guidance:

- It is OK (and preferred) to keep compound constructs readable over multiple
  lines with ShellFile.
- Preserve the same safety rules for user input: do not inline untrusted argv
  into control structures; pass via "$@" or validated/env values.

5) Make “no embedded newlines” a checked invariant

We enforce it in the builders:

 • In debug: debug_assert! that fragments/lines have no newlines.
 • In release: return an io::Error when a newline or NUL is present.

This turns silent regressions into immediate failures with a clear message.

6) Centralize quoting rules

We have shell_escape and shell_join. The builder should be used only for the
control script; user args should still go through shell_join/shell_escape when
needed for logging or previews. Keeping those responsibilities separate avoids
injection bugs.

7) Common failure mode to avoid (dash syntax errors)

Bad (split across fragments, becomes “then; …” due to “; ” joiner):
- push("if cond"); push("then"); push("do_x"); push("fi")

Good (single fragment):
- push("if cond; then do_x; fi")

8) Quick decision checklist (LLM-friendly)

- Will this run via sh -c? → Use ShellScript.
- Will this be written to disk and executed later? → Use ShellFile.
- Does a fragment end with control keywords (then, do, {, case)? → Do NOT split
  the construct; keep it in a single ShellScript fragment.
- Any user input involved? → Do NOT embed in control script text; pass via "$@"
  and/or validated env, and use shell_escape/shell_join only for logs or previews.
