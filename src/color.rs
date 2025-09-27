#![allow(clippy::module_name_repetitions)]
//! Color mode configuration and ANSI painting helpers.

use clap::ValueEnum;
use once_cell::sync::OnceCell;

/// Color mode and helpers (extracted to module)
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

static COLOR_MODE: OnceCell<ColorMode> = OnceCell::new();

pub fn set_color_mode(mode: ColorMode) {
    let _ = COLOR_MODE.set(mode);
}

fn parse_color_mode(s: &str) -> Option<ColorMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(ColorMode::Auto),
        "always" | "on" | "true" | "yes" => Some(ColorMode::Always),
        "never" | "off" | "false" | "no" => Some(ColorMode::Never),
        _ => None,
    }
}

fn env_color_mode_pref() -> Option<ColorMode> {
    std::env::var("AIFO_CODER_COLOR")
        .ok()
        .and_then(|v| parse_color_mode(&v))
}

fn no_color_env() -> bool {
    // Per https://no-color.org/
    std::env::var("NO_COLOR").is_ok()
}

fn color_enabled_for(is_tty: bool) -> bool {
    // 1) Environment override wins over NO_COLOR and TTY checks
    if let Some(env_mode) = env_color_mode_pref() {
        return match env_mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => is_tty,
        };
    }
    // 2) Programmatic override via set_color_mode
    if let Some(mode) = COLOR_MODE.get().copied() {
        return match mode {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => is_tty,
        };
    }
    // 3) Respect NO_COLOR only when no overrides are present
    if no_color_env() {
        return false;
    }
    // 4) Default: auto (TTY)
    is_tty
}

pub fn color_enabled_stdout() -> bool {
    color_enabled_for(atty::is(atty::Stream::Stdout))
}

pub fn color_enabled_stderr() -> bool {
    color_enabled_for(atty::is(atty::Stream::Stderr))
}

/// Wrap string with ANSI color code when enabled; otherwise return unchanged.
pub fn paint(enabled: bool, code: &str, s: &str) -> String {
    if enabled {
        format!("{code}{s}\x1b[0m")
    } else {
        s.to_string()
    }
}

/// Minimal logging helpers for consistent, color-aware stderr output without changing message text.
/// Use only when the message strings are fully identical to existing prints.
pub fn log_info_stderr(use_color: bool, msg: &str) {
    eprintln!("{}", paint(use_color, "\x1b[36;1m", msg));
}

pub fn log_warn_stderr(use_color: bool, msg: &str) {
    eprintln!("{}", paint(use_color, "\x1b[33m", msg));
}

pub fn log_error_stderr(use_color: bool, msg: &str) {
    eprintln!("{}", paint(use_color, "\x1b[31;1m", msg));
}
