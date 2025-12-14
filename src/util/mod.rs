#![allow(clippy::module_name_repetitions)]
//! Small utilities: shell/json escaping, URL decoding, header parsing, simple tokenization.

pub mod docker_security;
pub mod fs;
pub mod id;
pub mod shell_script;

pub use shell_script::ShellScript;

/// Reject strings containing newline, carriage return, or NUL before embedding into a shell command.
///
/// Keep error text stable (tests/UX depend on it).
pub fn reject_newlines(s: &str, what: &str) -> Result<(), String> {
    if s.contains('\n') || s.contains('\r') || s.contains('\0') {
        Err(format!("refusing to execute {what}: contains newline"))
    } else {
        Ok(())
    }
}

pub fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|a| shell_escape(a))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        "''".to_string()
    } else if s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_=./:@".contains(c))
    {
        s.to_string()
    } else {
        let escaped = s.replace('\'', "'\"'\"'");
        format!("'{}'", escaped)
    }
}

pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn url_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = bytes[i + 1];
                let h2 = bytes[i + 2];
                let v1 = (h1 as char).to_digit(16);
                let v2 = (h2 as char).to_digit(16);
                if let (Some(a), Some(b)) = (v1, v2) {
                    out.push(((a << 4) + b) as u8 as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            _ => {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    out
}

pub fn find_crlfcrlf(buf: &[u8]) -> Option<usize> {
    if buf.len() < 4 {
        return None;
    }
    let pattern: &[u8; 4] = b"\r\n\r\n";
    buf.windows(4).position(|w| w == pattern)
}

/// Find end of HTTP headers, accepting either CRLF-CRLF or LF-LF separators.
/// Returns the index just after the header terminator when found.
pub fn find_header_end(buf: &[u8]) -> Option<usize> {
    if let Some(pos) = find_crlfcrlf(buf) {
        return Some(pos + 4);
    }
    buf.windows(2).position(|w| w == b"\n\n").map(|pos| pos + 2)
}

/// Extract outer single or double quotes if the whole string is wrapped.
pub fn strip_outer_quotes(s: &str) -> String {
    if s.len() >= 2 {
        let b = s.as_bytes();
        let first = b[0] as char;
        let last = b[s.len() - 1] as char;
        if (first == '\'' && last == '\'') || (first == '"' && last == '"') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

/// Minimal shell-like tokenizer supporting single and double quotes.
/// Does not support escapes; quotes preserve spaces.
pub fn shell_like_split_args(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for ch in s.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    out.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_escape_simple() {
        assert_eq!(shell_escape("abc-123_./:@"), "abc-123_./:@");
    }

    #[test]
    fn test_shell_escape_with_spaces_and_quotes() {
        assert_eq!(shell_escape("a b c"), "'a b c'");
        assert_eq!(shell_escape("O'Reilly"), "'O'\"'\"'Reilly'");
    }

    #[test]
    fn test_shell_join() {
        let args = vec!["a".to_string(), "b c".to_string(), "d".to_string()];
        assert_eq!(shell_join(&args), "a 'b c' d");
    }

    #[test]
    fn test_find_crlfcrlf_cases() {
        assert_eq!(find_crlfcrlf(b"\r\n\r\n"), Some(0));
        assert_eq!(find_crlfcrlf(b"abc\r\n\r\ndef"), Some(3));
        assert_eq!(find_crlfcrlf(b"abcdef"), None);
        assert_eq!(find_crlfcrlf(b"\r\n\r"), None);
    }

    #[test]
    fn test_strip_outer_quotes_variants() {
        assert_eq!(strip_outer_quotes("'abc'"), "abc");
        assert_eq!(strip_outer_quotes("\"abc\""), "abc");
        assert_eq!(strip_outer_quotes("'a b'"), "a b");
        assert_eq!(strip_outer_quotes("noquote"), "noquote");
        // Only strips if both ends match the same quote type
        assert_eq!(strip_outer_quotes("'mismatch\""), "'mismatch\"");
    }

    #[test]
    fn test_shell_like_split_args_quotes_and_spaces() {
        let args = shell_like_split_args("'a b' c \"d e\"");
        assert_eq!(
            args,
            vec!["a b".to_string(), "c".to_string(), "d e".to_string()]
        );

        let args2 = shell_like_split_args("  a   'b c'   d  ");
        assert_eq!(
            args2,
            vec!["a".to_string(), "b c".to_string(), "d".to_string()]
        );
    }

    #[test]
    fn test_url_decode_mixed() {
        assert_eq!(url_decode("a+b%20c%2F%3F%25"), "a b c/?%");
        assert_eq!(url_decode("%41%42%43"), "ABC");
        assert_eq!(url_decode("no-escapes_here~"), "no-escapes_here~");
    }
}
