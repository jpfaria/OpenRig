//! Filesystem-level validation for plugin packages.
//!
//! [`validate_manifest`](crate::validate::validate_manifest) checks the manifest
//! against itself. This module adds the second half: given a package root on
//! disk, verify that every file the manifest references actually exists.
//!
//! Splits into a separate function because the schema-level validation has no
//! filesystem dependency — handy in dry-runs, CI, and unit tests where you only
//! want to lint the YAML.
//!
//! Issue: #287

use std::path::{Path, PathBuf};

use crate::manifest::{Backend, Lv2Slot, PluginManifest};
use crate::validate::{validate_manifest, ValidationError};

/// Reasons a package fails validation.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("package root `{0}` is not a directory")]
    PackageRootNotADirectory(PathBuf),

    #[error("schema-level validation failed: {0}")]
    Validation(#[from] ValidationError),

    #[error("capture #{capture_index} file `{file}` not found in package root")]
    MissingCaptureFile { capture_index: usize, file: PathBuf },

    #[error("LV2 binary for slot {slot:?} (`{file}`) not found in package root")]
    MissingBinarySlot { slot: Lv2Slot, file: PathBuf },

    #[error("VST3 bundle directory `{bundle}` not found in package root")]
    MissingVst3Bundle { bundle: PathBuf },
}

/// Returns the [`Lv2Slot`] this binary should pick up at runtime, or [`None`]
/// when the host (OS, arch) tuple isn't covered by the supported slots.
pub fn current_platform_slot() -> Option<Lv2Slot> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => Some(Lv2Slot::MacosUniversal),
        ("windows", "x86_64") => Some(Lv2Slot::WindowsX86_64),
        ("windows", "aarch64") => Some(Lv2Slot::WindowsAarch64),
        ("linux", "x86_64") => Some(Lv2Slot::LinuxX86_64),
        ("linux", "aarch64") => Some(Lv2Slot::LinuxAarch64),
        _ => None,
    }
}

/// Validate a manifest against the package on disk.
///
/// Runs schema-level checks first, then walks every file the manifest
/// references and confirms it exists relative to `package_root`. The actual
/// LV2 `dlopen` smoke test is not performed here — that's intentionally a
/// separate concern (loader runtime), and it can only test the slot for the
/// host platform, not the others.
pub fn validate_package(
    package_root: &Path,
    manifest: &PluginManifest,
) -> Result<(), PackageError> {
    validate_manifest(manifest)?;

    if !package_root.is_dir() {
        return Err(PackageError::PackageRootNotADirectory(
            package_root.to_path_buf(),
        ));
    }

    match &manifest.backend {
        Backend::Native { .. } => {
            // Native plugins ship no files on disk — the runtime_id keys
            // into the in-binary `native_runtimes` table at startup.
        }
        Backend::Nam { captures, .. } | Backend::Ir { captures, .. } => {
            for (capture_index, capture) in captures.iter().enumerate() {
                let absolute = package_root.join(&capture.file);
                if !absolute.is_file() {
                    return Err(PackageError::MissingCaptureFile {
                        capture_index,
                        file: capture.file.clone(),
                    });
                }
            }
        }
        Backend::Lv2 { binaries, .. } => {
            for (slot, file) in binaries {
                let absolute = package_root.join(file);
                if !absolute.is_file() {
                    return Err(PackageError::MissingBinarySlot {
                        slot: *slot,
                        file: file.clone(),
                    });
                }
            }
        }
        Backend::Vst3 { bundle, .. } => {
            let absolute = package_root.join(bundle);
            if !absolute.is_dir() {
                return Err(PackageError::MissingVst3Bundle {
                    bundle: bundle.clone(),
                });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;
