pub fn path_pair(host: &std::path::Path, container: &str) -> std::ffi::OsString {
    // Special-case ~/.gitconfig: mount as .gitconfig-host.gitconfig so entrypoint can clone to writable ~/.gitconfig.
    if container == "/home/coder/.gitconfig" {
        std::ffi::OsString::from(format!(
            "{}:/home/coder/.gitconfig-host.gitconfig",
            host.display()
        ))
    } else {
        std::ffi::OsString::from(format!("{}:{container}", host.display()))
    }
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
