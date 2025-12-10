/*!
Routing and allowlists: map tools to sidecar kinds, allow/deny, and probe availability.

- route_tool_to_sidecar: primary mapping
- sidecar_allowlist: per-kind allowlist
- select_kind_for_tool: dynamic selection based on running sidecars and availability
*/
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::sidecar::sidecar_container_name;
use crate::container_runtime_path;

/// Common DEV tools shared across allowlists and shims (used to widen routing preferences).
const DEV_TOOLS: &[&str] = &[
    "make",
    "cmake",
    "ninja",
    "pkg-config",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "cc",
    "c++",
];

const ALLOW_RUST: &[&str] = &[
    "cargo",
    "rustc",
    "make",
    "cmake",
    "ninja",
    "pkg-config",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "cc",
    "c++",
];

const ALLOW_NODE: &[&str] = &[
    "node",
    "npm",
    "npx",
    "yarn",
    "pnpm",
    "deno",
    "bun",
    "tsc",
    "ts-node",
    "make",
    "cmake",
    "ninja",
    "pkg-config",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "cc",
    "c++",
];

const ALLOW_PYTHON: &[&str] = &[
    "python",
    "python3",
    "pip",
    "pip3",
    "uv",
    "uvx",
    "make",
    "cmake",
    "ninja",
    "pkg-config",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "cc",
    "c++",
];

const ALLOW_CCPP: &[&str] = &[
    "gcc",
    "g++",
    "cc",
    "c++",
    "clang",
    "clang++",
    "make",
    "cmake",
    "ninja",
    "pkg-config",
];

const ALLOW_GO: &[&str] = &[
    "go",
    "gofmt",
    "make",
    "cmake",
    "ninja",
    "pkg-config",
    "gcc",
    "g++",
    "clang",
    "clang++",
    "cc",
    "c++",
];

pub fn sidecar_allowlist(kind: &str) -> &'static [&'static str] {
    match kind {
        "rust" => ALLOW_RUST,
        "node" => ALLOW_NODE,
        "python" => ALLOW_PYTHON,
        "c-cpp" => ALLOW_CCPP,
        "go" => ALLOW_GO,
        _ => &[],
    }
}

/// Map a tool name to the sidecar kind.
pub fn route_tool_to_sidecar(tool: &str) -> &'static str {
    let t = tool.to_ascii_lowercase();
    match t.as_str() {
        // rust
        "cargo" | "rustc" => "rust",
        // node/typescript and related managers
        "node" | "npm" | "npx" | "yarn" | "pnpm" | "deno" | "bun" | "tsc" | "ts-node" => "node",
        // python and uv/uvx tools
        "python" | "python3" | "pip" | "pip3" | "uv" | "uvx" => "python",
        // c/c++
        "gcc" | "g++" | "clang" | "clang++" | "make" | "cmake" | "ninja" | "pkg-config" => "c-cpp",
        // go
        "go" | "gofmt" => "go",
        _ => "node",
    }
}

// Determine if a tool is a generic build tool that may exist across sidecars
fn is_dev_tool(tool: &str) -> bool {
    DEV_TOOLS.contains(&tool)
}

// Best-effort: check if a container with the given name exists (running or created)
pub fn container_exists(name: &str) -> bool {
    if let Ok(runtime) = container_runtime_path() {
        return Command::new(&runtime)
            .arg("inspect")
            .arg(name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    false
}

// Best-effort: check if tool is available inside the given container (cached by caller)
fn tool_available_in(name: &str, tool: &str, timeout_secs: u64) -> bool {
    if let Ok(runtime) = container_runtime_path() {
        let mut cmd = Command::new(&runtime);
        cmd.arg("exec")
            .arg(name)
            .arg("/bin/sh")
            .arg("-c")
            .arg(format!("command -v {} >/dev/null 2>&1", tool))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Run with a simple timeout by spawning and joining
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let st = cmd.status();
            let _ = tx.send(st.ok().map(|s| s.success()).unwrap_or(false));
        });
        // Use a small default when no global timeout is configured, so we can cache availability.
        let probe_secs = if timeout_secs == 0 { 2 } else { timeout_secs };
        if let Ok(ok) = rx.recv_timeout(Duration::from_secs(probe_secs)) {
            return ok;
        }
    }
    false
}

// Preferred sidecars for a given tool (in order)
fn preferred_kinds_for_tool(tool: &str) -> Vec<&'static str> {
    let t = tool.to_ascii_lowercase();
    if is_dev_tool(&t) {
        vec!["c-cpp", "rust", "go", "node", "python"]
    } else {
        vec![route_tool_to_sidecar(&t)]
    }
}

// Select the best sidecar kind for tool based on running containers and availability; fallback to primary preference.
pub fn select_kind_for_tool(
    session_id: &str,
    tool: &str,
    timeout_secs: u64,
    cache: &mut HashMap<(String, String), bool>,
) -> String {
    let prefs = preferred_kinds_for_tool(tool);
    for k in &prefs {
        let name = sidecar_container_name(k, session_id);
        if !container_exists(&name) {
            continue;
        }
        let key = (name.clone(), tool.to_ascii_lowercase());
        let ok = if let Some(cached) = cache.get(&key) {
            *cached
        } else {
            let avail = tool_available_in(&name, tool, timeout_secs);
            cache.insert(key.clone(), avail);
            avail
        };
        if ok {
            return (*k).to_string();
        }
    }
    // fallback to first preference (may not be running; higher layers handle errors)
    prefs[0].to_string()
}
