//! Responsibility: keeps the app-wide library of saved looper takes.
//! #827 — a take is a looper mixdown the user named and kept.
//!
//! Takes live in ONE folder of the system data dir (never beside a project),
//! so every project sees them; each is a plain interleaved-stereo wav, which
//! is exactly what the DI plays through `DiLoopSource::File`. Plain file I/O
//! on the control thread — never the audio thread.

use std::path::{Path, PathBuf};

/// Why a take could not be saved. Typed, so a frontend can tell the user
/// WHICH of these happened instead of echoing an I/O string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeSaveError {
    /// The name has nothing left once made disk-safe.
    EmptyName,
    /// A take with this file name already exists; it is never overwritten.
    NameTaken(String),
    /// The looper holds no audio (or no looper store is running).
    NothingRecorded,
    /// The file could not be written.
    Io(String),
}

impl std::fmt::Display for TakeSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => write!(f, "a take needs a name"),
            Self::NameTaken(file) => write!(f, "a take named {file} already exists"),
            Self::NothingRecorded => write!(f, "this looper holds no recorded audio"),
            Self::Io(err) => write!(f, "could not write the take: {err}"),
        }
    }
}

impl std::error::Error for TakeSaveError {}

const EXTENSION: &str = ".wav";

/// Disk-safe file name for a take called `name`: anything that could leave the
/// folder or confuse a file system becomes `-`, and `.wav` is appended once.
pub fn take_file_name(name: &str) -> Result<String, TakeSaveError> {
    let trimmed = name.trim();
    let stem = strip_extension(trimmed).unwrap_or(trimmed);
    let safe: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let safe = safe.trim_matches(|c: char| c == '-' || c.is_whitespace());
    if safe.is_empty() {
        return Err(TakeSaveError::EmptyName);
    }
    Ok(format!("{safe}{EXTENSION}"))
}

/// `name` without a trailing `.wav` (any case); `None` when it has none.
fn strip_extension(name: &str) -> Option<&str> {
    let cut = name.len().checked_sub(EXTENSION.len())?;
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(EXTENSION))
        .then(|| &name[..cut])
}

/// Write a take into `dir` (created on demand) and return its path. An
/// existing take is never overwritten: the file is opened create-new, so two
/// saves racing on one name cannot clobber each other either.
pub fn save_take(
    dir: &Path,
    name: &str,
    pcm: &[f32],
    sample_rate: u32,
) -> Result<PathBuf, TakeSaveError> {
    let file_name = take_file_name(name)?;
    if pcm.is_empty() {
        return Err(TakeSaveError::NothingRecorded);
    }
    std::fs::create_dir_all(dir).map_err(|e| TakeSaveError::Io(e.to_string()))?;
    let path = dir.join(&file_name);
    let file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(TakeSaveError::NameTaken(file_name));
        }
        Err(e) => return Err(TakeSaveError::Io(e.to_string())),
    };
    let written =
        crate::looper_audio::write_loop_samples(std::io::BufWriter::new(file), pcm, sample_rate);
    if let Err(e) = written {
        // A half-written take would show up in every DI picker as a file that
        // does not decode.
        let _ = std::fs::remove_file(&path);
        return Err(TakeSaveError::Io(e.to_string()));
    }
    Ok(path)
}

/// Every take in `dir`, in name order. A folder that does not exist yet is an
/// empty library, not an error.
pub fn list_takes(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut takes: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        })
        .collect();
    takes.sort();
    takes
}

/// Why a take could not be deleted. Typed like [`TakeSaveError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TakeDeleteError {
    /// The name is not a plain file name inside the library.
    InvalidName,
    /// No take with this file name is in the library.
    NotFound(String),
    /// The file could not be removed.
    Io(String),
}

impl std::fmt::Display for TakeDeleteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName => write!(f, "not a take of the library"),
            Self::NotFound(file) => write!(f, "no take named {file}"),
            Self::Io(err) => write!(f, "could not delete the take: {err}"),
        }
    }
}

impl std::error::Error for TakeDeleteError {}

/// The path of the take `name` (`.wav` optional) in `dir`. Only a
/// plain file name resolves — a separator, `..` or an absolute path is refused
/// before the file system is asked anything, so nothing outside the library
/// can ever be addressed.
pub fn resolve_take(dir: &Path, name: &str) -> Result<PathBuf, TakeDeleteError> {
    let trimmed = name.trim();
    let (stem, file) = match strip_extension(trimmed) {
        Some(stem) => (stem, trimmed.to_string()),
        None => (trimmed, format!("{trimmed}{EXTENSION}")),
    };
    let mut components = Path::new(stem).components();
    let plain = matches!(
        (components.next(), components.next()),
        (Some(std::path::Component::Normal(_)), None)
    );
    if !plain || stem.contains(['/', '\\']) {
        return Err(TakeDeleteError::InvalidName);
    }
    let path = dir.join(&file);
    if !path.is_file() {
        return Err(TakeDeleteError::NotFound(file));
    }
    Ok(path)
}

/// Delete the take `name` from `dir`, returning the path it had.
pub fn delete_take(dir: &Path, name: &str) -> Result<PathBuf, TakeDeleteError> {
    let path = resolve_take(dir, name)?;
    std::fs::remove_file(&path).map_err(|e| TakeDeleteError::Io(e.to_string()))?;
    Ok(path)
}

#[cfg(test)]
#[path = "looper_take_library_tests.rs"]
mod tests;
