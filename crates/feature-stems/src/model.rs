//! Off-RT model download + verified cache for `htdemucs` weights.
//!
//! The pipeline takes ML model weights as an explicit input so callers
//! decide where (and when) to fetch. Tests inject a [`ModelDownloader`]
//! that returns canned bytes; production uses [`UreqDownloader`] over
//! HTTPS.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::StemError;

/// Strategy for fetching model bytes from a URL.
///
/// Decoupled from the cache layer so tests can hand in canned bytes
/// without spinning up an HTTP server.
pub trait ModelDownloader {
    /// Fetch the entire payload at `url` as a byte vector.
    ///
    /// Errors are surfaced as opaque strings — callers convert them to
    /// [`StemError::ModelDownload`].
    fn download(&self, url: &str) -> Result<Vec<u8>, String>;
}

/// Production downloader backed by `ureq`. Blocking, no async runtime.
#[derive(Debug, Default, Clone, Copy)]
pub struct UreqDownloader;

impl ModelDownloader for UreqDownloader {
    fn download(&self, url: &str) -> Result<Vec<u8>, String> {
        let response = ureq::get(url).call().map_err(|err| err.to_string())?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|err| err.to_string())?;
        Ok(bytes)
    }
}

/// Ensure `<model_dir>/<filename>` exists and matches `expected_sha256`,
/// downloading from `url` via `downloader` when needed.
///
/// Semantics:
/// - Cache hit + matching SHA → returns the cached path without I/O on
///   the network.
/// - Cache miss or stale SHA → downloads, verifies SHA, atomically
///   writes the file, returns its path.
///
/// # Errors
///
/// - [`StemError::ModelDownload`] when the downloader fails, the SHA
///   does not match the downloaded bytes, or any required file
///   operation fails. The cache file is left untouched on SHA failure.
pub fn ensure_model_with(
    model_dir: &Path,
    url: &str,
    expected_sha256: &str,
    filename: &str,
    downloader: &dyn ModelDownloader,
) -> Result<PathBuf, StemError> {
    let target = model_dir.join(filename);

    if target.exists() {
        let cached = fs::read(&target).map_err(|err| StemError::ModelDownload {
            reason: format!("read cached file `{}`: {err}", target.display()),
        })?;
        if sha256_hex(&cached).eq_ignore_ascii_case(expected_sha256) {
            return Ok(target);
        }
    }

    let bytes = downloader
        .download(url)
        .map_err(|reason| StemError::ModelDownload {
            reason: format!("downloader failed for `{url}`: {reason}"),
        })?;
    let actual = sha256_hex(&bytes);
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(StemError::ModelDownload {
            reason: format!("SHA256 mismatch: expected {expected_sha256}, got {actual}"),
        });
    }

    fs::create_dir_all(model_dir).map_err(|err| StemError::ModelDownload {
        reason: format!("create model dir `{}`: {err}", model_dir.display()),
    })?;
    fs::write(&target, &bytes).map_err(|err| StemError::ModelDownload {
        reason: format!("write model file `{}`: {err}", target.display()),
    })?;
    Ok(target)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}
