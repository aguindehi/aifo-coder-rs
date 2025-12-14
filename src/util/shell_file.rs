use std::io;

/// Builder for shell scripts that are written to disk and later executed (multi-line allowed).
///
/// Invariants:
/// - Each pushed line must not contain `\n`, `\r`, or `\0` (prevents Rust formatting from changing
///   runtime behavior).
/// - `build()` joins lines with `\n` and ensures a trailing newline when non-empty.
#[derive(Debug, Default)]
pub struct ShellFile {
    lines: Vec<String>,
}

impl ShellFile {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Push one logical line (no embedded newlines).
    pub fn push(&mut self, line: impl Into<String>) -> &mut Self {
        self.lines.push(line.into());
        self
    }

    pub fn extend<I>(&mut self, lines: I) -> &mut Self
    where
        I: IntoIterator<Item = String>,
    {
        for l in lines {
            self.lines.push(l);
        }
        self
    }

    pub fn build(&self) -> io::Result<String> {
        for (i, l) in self.lines.iter().enumerate() {
            if l.contains('\n') || l.contains('\r') || l.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("shell file line {i} contains a newline or NUL; use atomic lines"),
                ));
            }
        }

        if self.lines.is_empty() {
            return Ok(String::new());
        }

        let mut out = self.lines.join("\n");
        if !out.ends_with('\n') {
            out.push('\n');
        }
        debug_assert!(!out.contains('\0'));
        Ok(out)
    }
}
