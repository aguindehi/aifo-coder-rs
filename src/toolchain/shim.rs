pub(crate) const SHIM_TOOLS: &[&str] = &[
    "cargo",
    "rustc",
    "node",
    "npm",
    "npx",
    "yarn",
    "pnpm",
    "deno",
    "tsc",
    "ts-node",
    "python",
    "pip",
    "pip3",
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
    "go",
    "gofmt",
    "say",
    "uv",
    "uvx",
];

/// Expose shim tool list for tests and image checks.
pub fn shim_tool_names() -> &'static [&'static str] {
    SHIM_TOOLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shim_tool_names_smoke_contains_core_entries() {
        assert!(SHIM_TOOLS.contains(&"node"));
        assert!(SHIM_TOOLS.contains(&"python"));
        assert!(SHIM_TOOLS.contains(&"cargo"));
        assert!(SHIM_TOOLS.contains(&"uv"));
        assert!(SHIM_TOOLS.contains(&"uvx"));
    }
}
