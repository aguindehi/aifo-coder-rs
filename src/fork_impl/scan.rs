use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn session_dirs(base: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let rd = match fs::read_dir(base) {
        Ok(d) => d,
        Err(_) => return out,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.push(p);
        }
    }
    out
}

pub fn pane_dirs_for_session(session_dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir(session_dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    if name.starts_with("pane-") {
                        v.push(p);
                    }
                }
            }
        }
    }
    v
}

pub fn secs_since_epoch(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}
