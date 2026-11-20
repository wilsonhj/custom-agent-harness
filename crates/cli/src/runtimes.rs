//! Where the state directory lives. Runtime resolution is the engine's (`resolve_config`).

use std::path::{Path, PathBuf};

/// `~/Library/Application Support/eng`.
pub fn default_state_dir() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/Application Support/eng"))
}

/// The state directory, created mode 0700 if absent.
pub fn ensure_state_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}
