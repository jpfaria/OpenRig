//! Responsibility: owns the on-disk session log files (#1060).

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Sessions kept on disk; older ones are deleted at startup.
pub const SESSIONS_KEPT: usize = 10;

const PREFIX: &str = "openrig-";
const SUFFIX: &str = ".log";

/// Platform log directory: macOS `~/Library/Logs/OpenRig`, Windows
/// `%APPDATA%\OpenRig\logs`, Linux `~/.local/share/openrig/logs`.
pub fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return dirs::home_dir().map(|h| h.join("Library").join("Logs").join("OpenRig"));
    #[cfg(target_os = "windows")]
    return dirs::data_dir().map(|d| d.join("OpenRig").join("logs"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return dirs::data_local_dir().map(|d| d.join("openrig").join("logs"));
}

fn session_number(name: &str) -> Option<u64> {
    name.strip_prefix(PREFIX)?
        .strip_suffix(SUFFIX)?
        .parse()
        .ok()
}

/// Creates `openrig-<unix secs>.log` in `dir` and prunes all but the
/// newest `keep` sessions (the new one included).
pub fn open_session_log(dir: &Path, keep: usize) -> io::Result<(PathBuf, File)> {
    std::fs::create_dir_all(dir)?;
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("{PREFIX}{secs}{SUFFIX}"));
    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let mut sessions: Vec<(u64, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| session_number(&e.file_name().to_string_lossy()).map(|n| (n, e.path())))
        .collect();
    sessions.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, old) in sessions.into_iter().skip(keep) {
        let _ = std::fs::remove_file(old);
    }
    Ok((path, file))
}
