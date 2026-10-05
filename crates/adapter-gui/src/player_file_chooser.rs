//! Responsibility: asks the user for a backing-track file anywhere on disk.
//!
//! The native dialog lives in its own file, like the DI loop chooser, so the
//! panel wiring stays free of `rfd`.

use std::path::PathBuf;

use application::player_library::BACKING_TRACK_EXTENSIONS;
use rfd::FileDialog;

/// The file the user picked, or `None` when the dialog was cancelled.
pub(crate) fn choose_backing_track() -> Option<PathBuf> {
    FileDialog::new()
        .add_filter("Audio", &BACKING_TRACK_EXTENSIONS)
        .pick_file()
}
