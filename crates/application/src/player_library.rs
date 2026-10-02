//! Responsibility: lists the backing tracks the player can load.
//!
//! Two folders feed the list: the tracks bundled with the app and the user's
//! own backing-tracks folder (`paths.backing_tracks_path` in `config.yaml`).
//! The listing is flat: a track is a supported audio file directly inside one
//! of them.

use std::path::{Path, PathBuf};

/// File extensions the player can decode.
pub const BACKING_TRACK_EXTENSIONS: [&str; 6] = ["wav", "flac", "mp3", "ogg", "m4a", "aac"];

/// One loadable track.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BackingTrack {
    /// The file name without its extension.
    pub name: String,
    pub path: PathBuf,
    /// Shipped with the app rather than added by the user.
    pub bundled: bool,
}

/// Where the tracks live. A folder that is `None` or missing lists nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerLibraryDirs {
    pub bundled: Option<PathBuf>,
    pub user: Option<PathBuf>,
}

/// Whether `path` names a file the player can decode, judged by extension.
/// A hidden file (`.name.wav`) is never a track.
pub fn is_backing_track_file(path: &Path) -> bool {
    let visible = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| !stem.is_empty() && !stem.starts_with('.'));
    let supported = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            BACKING_TRACK_EXTENSIONS
                .iter()
                .any(|known| known.eq_ignore_ascii_case(ext))
        });
    visible && supported
}

/// Bundled tracks first, then the user's, each sorted by name ignoring case.
pub fn list_backing_tracks(dirs: &PlayerLibraryDirs) -> Vec<BackingTrack> {
    let mut tracks = list_folder(dirs.bundled.as_deref(), true);
    tracks.extend(list_folder(dirs.user.as_deref(), false));
    tracks
}

fn list_folder(dir: Option<&Path>, bundled: bool) -> Vec<BackingTrack> {
    let Some(entries) = dir.and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return Vec::new();
    };
    let mut tracks: Vec<BackingTrack> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_backing_track_file(path))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some(BackingTrack {
                name,
                path,
                bundled,
            })
        })
        .collect();
    tracks.sort_by_key(|track| track.name.to_lowercase());
    tracks
}

#[cfg(test)]
#[path = "player_library_tests.rs"]
mod tests;
