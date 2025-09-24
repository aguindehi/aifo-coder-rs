#![doc = "Warning utilities: color-aware printing and interactive prompt helpers.\n\
Unix single-key input uses stty in best-effort mode; if stty is absent or fails,\n\
errors are ignored and a sane fallback is attempted. Behavior and messages are preserved."]
#[cfg(unix)]
use std::io::Read;
use std::io::Write;

/// Print a standardized warning line to stderr (color-aware).
pub fn warn_print(msg: &str) {
    let use_err = crate::color_enabled_stderr();
    eprintln!(
        "{}",
        crate::paint(use_err, "\x1b[33;1m", &format!("warning: {}", msg))
    );
}

fn finish_prompt_line() {
    // End the prompt line and add an extra blank line for visual separation
    eprintln!();
    eprintln!();
}

// Platform-specific single-key input helpers extracted for clarity and reuse.

#[cfg(windows)]
fn warn_input_windows() -> bool {
    unsafe {
        #[link(name = "msvcrt")]
        extern "C" {
            fn _getch() -> i32;
        }
        let ch = _getch();
        let ch = (ch as u8) as char;
        if ch == 'q' || ch == 'Q' {
            finish_prompt_line();
            false
        } else {
            finish_prompt_line();
            true
        }
    }
}

#[cfg(unix)]
fn warn_input_unix() -> bool {
    // Save current stty state
    let saved = std::process::Command::new("stty")
        .arg("-g")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    // Best-effort: set non-canonical mode, no echo, 1-byte min
    let _ = std::process::Command::new("stty")
        .args(["-icanon", "min", "1", "-echo"])
        .status();

    let mut buf = [0u8; 1];
    let _ = std::io::stdin().read(&mut buf);

    // Restore previous stty state (or sane fallback)
    if let Some(state) = saved {
        let _ = std::process::Command::new("stty").arg(&state).status();
    } else {
        let _ = std::process::Command::new("stty").arg("sane").status();
    }

    let ch = buf[0] as char;
    if ch == 'q' || ch == 'Q' {
        finish_prompt_line();
        false
    } else {
        finish_prompt_line();
        true
    }
}

#[cfg(not(any(unix, windows)))]
fn warn_input_fallback() -> bool {
    // Fallback: line-based input (non-tty or platforms without single-key support)
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
    finish_prompt_line();
    let c = s.trim().chars().next().unwrap_or('\n');
    c != 'q' && c != 'Q'
}

/// Print warning lines and, when interactive, prompt the user to continue or abort.
/// Returns true to continue, false to abort.
pub fn warn_prompt_continue_or_quit(lines: &[&str]) -> bool {
    let use_err = crate::color_enabled_stderr();
    for l in lines {
        eprintln!(
            "{}",
            crate::paint(use_err, "\x1b[33;1m", &format!("warning: {}", l))
        );
    }

    // Only prompt when interactive and not disabled by env/CI
    let interactive = atty::is(atty::Stream::Stdin) && atty::is(atty::Stream::Stderr);
    let disabled = std::env::var("AIFO_CODER_NO_WARN_PAUSE").ok().as_deref() == Some("1")
        || std::env::var("CI").ok().as_deref() == Some("1");
    if !interactive || disabled {
        return true;
    }

    eprint!(
        "{}",
        crate::paint(
            use_err,
            "\x1b[90m",
            "Press Enter to continue, or 'q' to abort: "
        )
    );
    let _ = std::io::stderr().flush();

    #[cfg(windows)]
    {
        warn_input_windows()
    }

    #[cfg(unix)]
    {
        warn_input_unix()
    }

    #[cfg(not(any(unix, windows)))]
    {
        warn_input_fallback()
    }
}
