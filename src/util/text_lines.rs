use std::io;

/// Builder for multi-line text content assembled from single-line fragments.
///
/// This is the general-purpose analogue of `ShellFile` (which is specifically for shell scripts
/// written to disk) and mirrors the builder shape used by `ShellScript`/`ShellFile`.
///
/// Invariants:
/// - Each pushed line must not contain `\n`, `\r`, or `\0`.
/// - `build_lf()` joins lines with `\n` and ensures a trailing `\n` when non-empty.
/// - `build_crlf()` joins lines with `\r\n` and ensures a trailing `\r\n` when non-empty.
#[derive(Debug, Default)]
pub struct TextLines {
    lines: Vec<String>,
}

impl TextLines {
    pub fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Push one logical line (no embedded CR/LF/NUL).
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

    pub fn build_lf(&self) -> io::Result<String> {
        self.build_with_sep("\n")
    }

    pub fn build_crlf(&self) -> io::Result<String> {
        self.build_with_sep("\r\n")
    }

    fn build_with_sep(&self, sep: &str) -> io::Result<String> {
        for (i, l) in self.lines.iter().enumerate() {
            if l.contains('\n') || l.contains('\r') || l.contains('\0') {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("text line {i} contains a newline or NUL; use atomic lines"),
                ));
            }
        }

        if self.lines.is_empty() {
            return Ok(String::new());
        }

        let mut out = self.lines.join(sep);
        out.push_str(sep);

        debug_assert!(!out.contains('\0'));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_lf_empty_is_empty() {
        let tl = TextLines::new();
        assert_eq!(tl.build_lf().unwrap(), "");
    }

    #[test]
    fn build_lf_appends_trailing_newline() {
        let mut tl = TextLines::new();
        tl.push("a").push("b");
        assert_eq!(tl.build_lf().unwrap(), "a\nb\n");
    }

    #[test]
    fn build_crlf_appends_trailing_crlf() {
        let mut tl = TextLines::new();
        tl.push("a").push("b");
        assert_eq!(tl.build_crlf().unwrap(), "a\r\nb\r\n");
    }

    #[test]
    fn rejects_newline_in_line() {
        let mut tl = TextLines::new();
        tl.push("ok").push("bad\nline");
        assert!(tl.build_lf().is_err());
    }

    #[test]
    fn rejects_carriage_return_in_line() {
        let mut tl = TextLines::new();
        tl.push("bad\rline");
        assert!(tl.build_lf().is_err());
    }

    #[test]
    fn rejects_nul_in_line() {
        let mut tl = TextLines::new();
        tl.push("bad\0line");
        assert!(tl.build_lf().is_err());
    }
}
