//! Responsibility: resolves the path of a project file.

use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;

pub(crate) fn canonical_project_path(path: &PathBuf) -> Result<PathBuf> {
    // Do NOT call path.exists() here — blocks on disconnected network volumes.
    // fs::canonicalize resolves symlinks and normalises the path without blocking
    // for paths that exist on local storage; for paths that don't exist it errors
    // and we fall back to the raw path.
    if let Ok(c) = fs::canonicalize(path) {
        return Ok(without_verbatim_prefix(c));
    }
    if path.is_absolute() {
        return Ok(path.clone());
    }
    Ok(env::current_dir()?.join(path))
}

pub(crate) fn parse_path_argument(flag: &str) -> Option<PathBuf> {
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

/// `path` without the `\\?\` verbatim prefix `fs::canonicalize` adds on
/// Windows, when it names a drive path.
pub(crate) fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let drive_path = path
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
        .filter(|rest| {
            let bytes = rest.as_bytes();
            bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
        })
        .map(PathBuf::from);
    drive_path.unwrap_or(path)
}

#[cfg(test)]
#[path = "project_path_tests.rs"]
mod tests;
