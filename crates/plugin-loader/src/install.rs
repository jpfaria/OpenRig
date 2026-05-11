//! First-launch extraction of the bundled plugins zip.
//!
//! The OpenRig installer ships a single `openrig-plugins.zip` produced
//! by the OpenRig-plugins repo (`scripts/bundle-into-openrig.sh`).
//! On first launch, we extract that zip into the per-user plugins
//! directory so [`crate::registry::init`] can discover them on disk.
//!
//! Re-extraction is skipped if the destination already contains at
//! least one `<backend>/<id>/manifest.yaml`. To force a re-install,
//! the user (or an upgrade script) wipes the destination first.

use std::fs::{self, File};
use std::io;
use std::path::Path;

/// Returns true if `plugins_root` already holds at least one extracted
/// package (any sub-`sub-dir/manifest.yaml`). Used to decide whether the
/// first-launch extraction step needs to run.
pub fn has_extracted_packages(plugins_root: &Path) -> bool {
    let Ok(backends) = fs::read_dir(plugins_root) else {
        return false;
    };
    for backend in backends.flatten() {
        let Ok(packages) = fs::read_dir(backend.path()) else {
            continue;
        };
        for pkg in packages.flatten() {
            if pkg.path().join("manifest.yaml").is_file() {
                return true;
            }
        }
    }
    false
}

/// Extract `bundle_zip` into `plugins_root` if and only if the
/// destination is empty of plugin packages. No-op when:
/// - `bundle_zip` does not exist (dev mode pointing at OpenRig-plugins
///   sources directly), or
/// - `plugins_root` already contains extracted packages.
///
/// Returns the number of files written, or 0 when skipped.
pub fn extract_bundle_if_needed(plugins_root: &Path, bundle_zip: &Path) -> anyhow::Result<usize> {
    if has_extracted_packages(plugins_root) {
        log::info!(
            "plugin bundle: {} already populated, skipping extraction",
            plugins_root.display()
        );
        return Ok(0);
    }
    if !bundle_zip.is_file() {
        log::info!(
            "plugin bundle: no zip at {} (dev mode), skipping extraction",
            bundle_zip.display()
        );
        return Ok(0);
    }
    log::info!(
        "plugin bundle: extracting {} -> {}",
        bundle_zip.display(),
        plugins_root.display()
    );
    fs::create_dir_all(plugins_root)?;
    let written = extract_zip(bundle_zip, plugins_root)?;
    log::info!("plugin bundle: extracted {} file(s)", written);
    Ok(written)
}

fn extract_zip(zip_path: &Path, dest: &Path) -> anyhow::Result<usize> {
    let file = File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut written = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            // Reject unsafe paths (`..`, absolute, etc.).
            continue;
        };
        let target = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&target)?;
        io::copy(&mut entry, &mut out)?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
