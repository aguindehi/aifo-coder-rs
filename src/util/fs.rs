pub fn path_pair(host: &std::path::Path, container: &str) -> std::ffi::OsString {
    std::ffi::OsString::from(format!("{}:{container}", host.display()))
}

/// Ensure a file exists by creating parent directories as needed.
pub fn ensure_file_exists(p: &std::path::Path) -> std::io::Result<()> {
    if !p.exists() {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(p)?;
    }
    Ok(())
}
