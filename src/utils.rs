use std::path::PathBuf;

/// Returns the directory where the executable is located.
/// All app files (config, history, logs, assets) should be relative to this.
pub fn get_exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}
