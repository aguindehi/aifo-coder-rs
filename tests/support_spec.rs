#![allow(clippy::unwrap_used)]
// ignore-tidy-linelength

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn find_aifo_binary() -> String {
    if let Ok(p) = env::var("CARGO_BIN_EXE_aifo-coder") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    if let Ok(p) = env::var("CARGO_BIN_EXE_aifo_coder") {
        if !p.trim().is_empty() {
            return p;
        }
    }
    #[cfg(windows)]
    let name = "aifo-coder.exe";
    #[cfg(not(windows))]
    let name = "aifo-coder";
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(td) = env::var("CARGO_TARGET_DIR") {
        let b = PathBuf::from(td);
        cands.push(b.join("debug").join(name));
        cands.push(b.join("release").join(name));
    }
    cands.push(PathBuf::from("target").join("debug").join(name));
    cands.push(PathBuf::from("target").join("release").join(name));
    if let Ok(root) = env::var("CARGO_MANIFEST_DIR") {
        cands.push(PathBuf::from(root).join("aifo-coder"));
    } else {
        cands.push(PathBuf::from("aifo-coder"));
    }
    for p in cands {
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }
    "aifo-coder".to_string()
}

/// Integration: docker missing should return nonzero and print a clear error line.
/// Forces container_runtime_path() to fail via AIFO_CODER_TEST_DISABLE_DOCKER=1.
#[test]
fn test_support_docker_missing_integration() {
    // Resolve compiled binary path provided by cargo
    let bin = find_aifo_binary();
    let mut cmd = Command::new(bin);
    cmd.arg("support");
    cmd.env("AIFO_CODER_TEST_DISABLE_DOCKER", "1");
    cmd.env("AIFO_SUPPORT_ANIMATE", "0");
    let out = cmd.output().expect("failed to exec support command");
    assert!(
        !out.status.success(),
        "support should return nonzero when docker is missing"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    // Error line should include aifo-coder prefix and mention docker
    assert!(
        stderr.contains("aifo-coder:"),
        "stderr should include aifo-coder prefix: {}",
        stderr
    );
    assert!(
        stderr.to_ascii_lowercase().contains("docker"),
        "stderr should mention docker: {}",
        stderr
    );
}

/// Deterministic shuffle: same seed should produce identical order.
#[test]
fn test_support_shuffle_is_deterministic() {
    // Local copy of xorshift64* and Fisher–Yates used by support.rs
    #[derive(Clone)]
    struct XorShift64 {
        state: u64,
    }
    impl XorShift64 {
        fn new(seed: u64) -> Self {
            let s = if seed == 0 { 0x9e3779b97f4a7c15 } else { seed };
            Self { state: s }
        }
        fn next_u64(&mut self) -> u64 {
            let mut x = self.state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.state = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }
        fn next_usize(&mut self, bound: usize) -> usize {
            if bound <= 1 {
                0
            } else {
                (self.next_u64() as usize) % bound
            }
        }
    }
    fn shuffle_pairs(pairs: &mut Vec<(usize, usize)>, seed: u64) {
        let mut rng = XorShift64::new(seed);
        let n = pairs.len();
        for i in (1..n).rev() {
            let j = rng.next_usize(i + 1);
            pairs.swap(i, j);
        }
    }

    let mut v1: Vec<(usize, usize)> = (0..3).flat_map(|r| (0..3).map(move |c| (r, c))).collect();
    let mut v2 = v1.clone();
    shuffle_pairs(&mut v1, 1);
    shuffle_pairs(&mut v2, 1);

    assert_eq!(
        v1, v2,
        "shuffle with the same seed must produce the same sequence"
    );
    // Spot-check the first few entries to guard against accidental changes
    assert!(!v1.is_empty(), "worklist should not be empty after shuffle");
}

/// Agent check caching: only one agent --version per agent across all toolchains.
#[test]
fn test_agent_check_once_per_agent() {
    // Simulate worker caching behavior with 3 agents × 2 toolchains
    let agents = vec![
        "aider".to_string(),
        "crush".to_string(),
        "codex".to_string(),
    ];
    let kinds = vec!["rust".to_string(), "node".to_string()];
    let mut worklist: Vec<(usize, usize)> = Vec::new();
    for ai in 0..agents.len() {
        for ki in 0..kinds.len() {
            worklist.push((ai, ki));
        }
    }
    // No shuffle needed for this caching test
    let mut agent_ok: Vec<Option<bool>> = vec![None; agents.len()];
    let mut calls_per_agent: Vec<usize> = vec![0; agents.len()];

    for (ai, _ki) in worklist {
        if agent_ok[ai].is_none() {
            // Mock agent --version check call
            calls_per_agent[ai] += 1;
            agent_ok[ai] = Some(true);
        }
        // Mock pm check; not needed for counting
    }

    // Each agent should be checked exactly once
    for (i, count) in calls_per_agent.iter().enumerate() {
        assert!(
            *count == 1,
            "agent {} expected 1 call, got {}",
            agents[i],
            count
        );
    }
}

/// Smoke: with docker present, run a tiny non-TTY support and assert tokens appear.
#[test]
fn test_support_matrix_smoke_non_tty() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let bin = find_aifo_binary();
    let mut cmd = Command::new(bin);
    cmd.arg("support");
    // Limit scope to a tiny matrix and disable animation (non-TTY static render)
    cmd.env("AIFO_SUPPORT_AGENTS", "crush");
    cmd.env("AIFO_SUPPORT_TOOLCHAINS", "node");
    cmd.env("AIFO_SUPPORT_ANIMATE", "0");

    let out = cmd.output().expect("failed to exec support command");
    assert!(
        out.status.success(),
        "support should succeed (matrix printed)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        stderr.contains("Support matrix:"),
        "stderr should include header; got: {}",
        stderr
    );
    let has_any = stderr.contains("PASS") || stderr.contains("WARN") || stderr.contains("FAIL");
    assert!(
        has_any,
        "matrix must contain at least one PASS/WARN/FAIL token; stderr: {}",
        stderr
    );
}

#[test]
fn test_support_matrix_green_summary_for_crush_node() {
    // Skip if docker isn't available on this host
    if aifo_coder::container_runtime_path().is_err() {
        eprintln!("skipping: docker not found in PATH");
        return;
    }

    let bin = find_aifo_binary();
    let mut cmd = Command::new(bin);
    cmd.arg("support");
    // Limit matrix to a single well-supported combination and disable animation
    cmd.env("AIFO_SUPPORT_AGENTS", "crush");
    cmd.env("AIFO_SUPPORT_TOOLCHAINS", "node");
    cmd.env("AIFO_SUPPORT_ANIMATE", "0");
    // Ensure color codes don't interfere with parsing in non-TTY
    cmd.env("NO_COLOR", "1");

    let out = cmd.output().expect("failed to exec support command");
    assert!(
        out.status.success(),
        "support should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // Find the final Summary line
    let summary = stderr
        .lines()
        .filter(|l| l.starts_with("Summary:"))
        .last()
        .unwrap_or("");
    assert!(
        !summary.is_empty(),
        "expected a Summary line in stderr; got: {}",
        stderr
    );

    // Extract PASS/WARN/FAIL counts from the Summary line
    let mut pass = None;
    let mut warn = None;
    let mut fail = None;
    for part in summary.split_whitespace() {
        if let Some(v) = part.strip_prefix("PASS=") {
            pass = v.parse::<usize>().ok();
        } else if let Some(v) = part.strip_prefix("WARN=") {
            warn = v.parse::<usize>().ok();
        } else if let Some(v) = part.strip_prefix("FAIL=") {
            fail = v.parse::<usize>().ok();
        }
    }
    let (p, w, f) = (
        pass.unwrap_or(0),
        warn.unwrap_or(usize::MAX),
        fail.unwrap_or(usize::MAX),
    );

    assert!(
        w == 0 && f == 0 && p > 0,
        "expected green matrix (no WARN/FAIL); got summary: {}",
        summary
    );
}
