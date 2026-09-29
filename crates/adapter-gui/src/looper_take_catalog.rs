//! Responsibility: keeps the GUI's list of saved looper takes current.
//! #827 — the DI picker's view of the app-wide take library.
//!
//! The meter tick rebuilds every chain's DI source list, so it must not list
//! the folder on every pass. The catalog re-lists only when the folder's
//! modification time moves (a take added, renamed or deleted — by the editor,
//! by an MCP client or by hand in the file manager). GUI thread only.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// A folder touched this recently is re-listed even when its modification time
/// did not move: file systems with a coarse clock (FAT, some network shares)
/// can stamp two changes a second apart with the same time.
const COARSE_CLOCK_WINDOW: Duration = Duration::from_secs(2);

pub struct TakeCatalog {
    dir: PathBuf,
    /// The folder's modification time at the last listing; `None` ⇒ never
    /// listed, or the folder did not exist.
    stamp: Option<SystemTime>,
    listed: bool,
    takes: Vec<PathBuf>,
}

impl TakeCatalog {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            stamp: None,
            listed: false,
            takes: Vec::new(),
        }
    }

    /// The takes as the folder holds them now. One `stat` per call; the folder
    /// is listed again only when it changed.
    pub fn takes(&mut self) -> &[PathBuf] {
        let stamp = std::fs::metadata(&self.dir).and_then(|m| m.modified()).ok();
        let recent = stamp
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < COARSE_CLOCK_WINDOW);
        if !self.listed || stamp != self.stamp || recent {
            self.takes = application::looper_take_library::list_takes(&self.dir);
            self.stamp = stamp;
            self.listed = true;
        }
        &self.takes
    }
}

#[cfg(test)]
#[path = "looper_take_catalog_tests.rs"]
mod tests;
