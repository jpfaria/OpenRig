//! Responsibility: provides the log file sink of the windowed Windows GUI.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Where the log of the current session goes, under the user data root.
pub fn log_file_path(data_root: &Path) -> PathBuf {
    data_root.join("logs").join("openrig.log")
}

/// Opens `path` for this session's log, creating its folder. The previous
/// session's log is replaced: the file holds the run a problem was seen in.
pub fn open_log_file(path: &Path) -> std::io::Result<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    File::create(path)
}

/// Writes every byte to two sinks. A failing `first` (stderr of a windowed
/// process started without a console) never keeps a byte from `second`.
pub struct TeeWriter<A: Write, B: Write> {
    first: A,
    second: B,
}

impl<A: Write, B: Write> TeeWriter<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }

    #[cfg(test)]
    pub fn into_inner(self) -> (A, B) {
        (self.first, self.second)
    }
}

impl<A: Write, B: Write> Write for TeeWriter<A, B> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let _ = self.first.write_all(bytes);
        self.second.write_all(bytes)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.first.flush();
        self.second.flush()
    }
}

#[cfg(test)]
#[path = "log_file_tests.rs"]
mod tests;
