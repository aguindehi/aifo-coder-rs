use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Finding {
    path: PathBuf,
    line_1: usize,
    msg: String,
}

fn is_rust_path(p: &Path) -> bool {
    matches!(p.extension().and_then(|s| s.to_str()), Some("rs")) || p.file_name() == Some("build.rs".as_ref())
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name, ".git" | "target" | "dist" | "build" | "node_modules" | ".buildx-cache")
}

fn collect_rust_files(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for ent in fs::read_dir(&dir)? {
            let ent = ent?;
            let path = ent.path();
            let ft = ent.file_type()?;
            if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    if should_skip_dir(name) {
                        continue;
                    }
                }
                stack.push(path);
            } else if ft.is_file() {
                if is_rust_path(&path) {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Returns true if there is a source-level line-continuation string (`"\` at EOL),
/// ignoring whitespace after the backslash.
fn has_continuation_string(line: &str) -> bool {
    // Scan for "... \ <spaces><EOL>".
    // We treat any `"\` that is not in a comment as a continuation candidate.
    // This is conservative and intended as a regression guard.
    let mut in_string = false;
    let mut escape = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if !in_string {
            if c == '/' && chars.peek() == Some(&'/') {
                return false;
            }
            if c == '"' {
                in_string = true;
                escape = false;
            }
            continue;
        }

        if escape {
            escape = false;
            continue;
        }

        match c {
            '\\' => {
                if chars.peek() == Some(&'\n') {
                    // Impossible in a single line, but keep structure symmetric.
                    return true;
                }
                escape = true;
                // Check EOL continuation: `"\` at end of line (allow trailing spaces/tabs).
                if chars.peek().is_none() {
                    return true;
                }
            }
            '"' => {
                in_string = false;
            }
            _ => {}
        }
    }
    false
}

/// Check for a string token that spans multiple *source* lines.
/// This is a light-weight lexer, not a full Rust parser:
/// - Handles normal strings: "..."
/// - Handles raw strings: r#"..."#, r##"..."##, etc.
/// - Ignores content inside line comments (`// ...`) and block comments (`/* ... */`).
fn check_file(path: &Path, text: &str) -> Vec<Finding> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();

    let mut i: usize = 0;
    let mut line_1: usize = 1;

    let mut in_line_comment = false;
    let mut block_comment_depth: usize = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'\n' {
            line_1 += 1;
            in_line_comment = false;
            i += 1;
            continue;
        }

        if in_line_comment {
            i += 1;
            continue;
        }

        if block_comment_depth > 0 {
            if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                block_comment_depth += 1;
                i += 2;
                continue;
            }
            if b == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                block_comment_depth -= 1;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }

        // Enter comments
        if b == b'/' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'/' {
                in_line_comment = true;
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'*' {
                block_comment_depth = 1;
                i += 2;
                continue;
            }
        }

        // Continuation strings (best-effort; line-local).
        if b == b'"' {
            // We'll lex the string below, but also detect the explicit `"\` EOL pattern.
            // That pattern must be forbidden regardless of runtime content.
            if let Some(line_end) = text[i..].find('\n') {
                let line = &text[i..i + line_end];
                if has_continuation_string(line) {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        line_1,
                        msg: r#"forbidden continuation string: `"\` at end of line"#.to_string(),
                    });
                }
            }
        }

        // Normal string: "..."
        if b == b'"' {
            let start_line = line_1;
            i += 1;
            let mut escape = false;
            while i < bytes.len() {
                let bb = bytes[i];
                if bb == b'\n' {
                    out.push(Finding {
                        path: path.to_path_buf(),
                        line_1: start_line,
                        msg: "forbidden multi-line string literal".to_string(),
                    });
                    // Recover: keep scanning; treat as if string continues until closing quote.
                    line_1 += 1;
                    i += 1;
                    continue;
                }
                if escape {
                    escape = false;
                    i += 1;
                    continue;
                }
                match bb {
                    b'\\' => {
                        escape = true;
                        i += 1;
                    }
                    b'"' => {
                        i += 1;
                        break;
                    }
                    _ => i += 1,
                }
            }
            continue;
        }

        // Raw string: r###" ... "###
        if b == b'r' && i + 1 < bytes.len() {
            let mut j = i + 1;
            let mut hashes: usize = 0;
            while j < bytes.len() && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                let start_line = line_1;
                j += 1; // after opening quote

                // Find closing: `"` + hashes of `#`
                while j < bytes.len() {
                    if bytes[j] == b'\n' {
                        out.push(Finding {
                            path: path.to_path_buf(),
                            line_1: start_line,
                            msg: "forbidden multi-line raw string literal".to_string(),
                        });
                        line_1 += 1;
                        j += 1;
                        continue;
                    }

                    if bytes[j] == b'"' {
                        let mut k = j + 1;
                        let mut found = true;
                        for _ in 0..hashes {
                            if k >= bytes.len() || bytes[k] != b'#' {
                                found = false;
                                break;
                            }
                            k += 1;
                        }
                        if found {
                            j = k;
                            break;
                        }
                    }
                    j += 1;
                }

                i = j;
                continue;
            }
        }

        i += 1;
    }

    out
}

fn main() -> io::Result<()> {
    let root = Path::new(".");
    let files = collect_rust_files(root)?;

    let mut findings: Vec<Finding> = Vec::new();
    for p in files {
        let Ok(text) = fs::read_to_string(&p) else {
            continue;
        };
        let mut f = check_file(&p, &text);
        findings.append(&mut f);
    }

    if findings.is_empty() {
        return Ok(());
    }

    eprintln!("error: forbidden Rust multi-line literals / continuation strings detected:");
    for f in findings {
        eprintln!("{}:{}: {}", f.path.display(), f.line_1, f.msg);
    }
    Err(io::Error::new(
        io::ErrorKind::Other,
        "tidy: multiline/continuation string literals found",
    ))
}
