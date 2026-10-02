//! Responsibility: lists the backing tracks the player can load.
//!
//! Two folders feed the list: the tracks bundled with the app and the user's
//! own backing-tracks folder (`paths.backing_tracks_path` in `config.yaml`).
//! A track is a supported audio file directly inside one of them or inside one
//! of their subfolders; the subfolder names its category.

use std::path::{Path, PathBuf};

use crate::player_track_category::TrackCategory;

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
    pub category: TrackCategory,
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

/// The files at the top of `dir` (no category) and those one folder down
/// (the folder's category), sorted by name ignoring case.
fn list_folder(dir: Option<&Path>, bundled: bool) -> Vec<BackingTrack> {
    let Some(dir) = dir else {
        return Vec::new();
    };
    let mut tracks = list_files(dir, bundled, TrackCategory::Other);
    for sub in subfolders(dir) {
        let name = sub.file_name().map(|n| n.to_string_lossy().into_owned());
        let category = TrackCategory::from_folder(name.as_deref().unwrap_or_default());
        tracks.extend(list_files(&sub, bundled, category));
    }
    tracks.sort_by_key(|track| track.name.to_lowercase());
    tracks
}

fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

fn list_files(dir: &Path, bundled: bool, category: TrackCategory) -> Vec<BackingTrack> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_backing_track_file(path))
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            Some(BackingTrack {
                name,
                path,
                bundled,
                category,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "player_library_tests.rs"]
mod tests;
