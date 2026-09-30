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
    let stem = match trimmed.len().checked_sub(EXTENSION.len()) {
        Some(cut)
            if trimmed.is_char_boundary(cut) && trimmed[cut..].eq_ignore_ascii_case(EXTENSION) =>
        {
            &trimmed[..cut]
        }
        _ => trimmed,
    };
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

#[cfg(test)]
#[path = "looper_take_library_tests.rs"]
mod tests;
