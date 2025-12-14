use std::io;

/// Builder for shell scripts that are executed via `/bin/sh -c`.
///
/// Invariants:
/// - Commands must not contain `\n` or `\r` (prevents Rust formatting from changing behavior).
/// - Commands are joined with `; ` into a single line suitable for `sh -c`.
///
/// Fragment boundary rule (important):
/// - Do not split compound shell constructs across fragments (e.g. `if/then/fi`, `for/do/done`,
///   `case/esac`). ShellScript joins fragments with `; `, which can produce invalid syntax if a
///   fragment ends at `then`/`do` boundaries (e.g. `then; ...`).
/// - Keep compound constructs in a single fragment, or rewrite them as a single-line construct
///   (`if ...; then ...; fi`) within one fragment.
#[derive(Debug, Default)]
pub struct ShellScript {
    parts: Vec<String>,
}

impl ShellScript {
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn push(&mut self, cmd: impl Into<String>) -> &mut Self {
        self.parts.push(cmd.into());
        self
    }

    pub fn extend<I>(&mut self, cmds: I) -> &mut Self
    where
        I: IntoIterator<Item = String>,
    {
        for c in cmds {
            self.parts.push(c);
        }
        self
    }

    pub fn build(&self) -> io::Result<String> {
        for (i, p) in self.parts.iter().enumerate() {
            if p.contains('\n') || p.contains('\r') || p.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "shell script fragment {i} contains a newline or NUL; use atomic fragments"
                    ),
                ));
            }
        }
        let out = self.parts.join("; ");
        debug_assert!(!out.contains('\n') && !out.contains('\r') && !out.contains('\0'));
        Ok(out)
    }
}
