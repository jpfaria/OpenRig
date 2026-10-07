//! Responsibility: turns one TONE3000 tone into an installed plugin package.
//!
//! Captures download into a hidden staging folder that the loader never
//! reads as a package (it has no manifest yet). The folder is renamed into
//! place and the manifest written last, through a temp file, so a scan
//! never sees half a package. Any failure removes what was written. An
//! update builds the new package the same way and swaps it in only when it
//! is complete.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::api_client::{all_models, Tone3000Api};
use super::api_enums::{Tone3000Architecture, Tone3000BlockType};
use super::api_types::{Model, Tone};
use super::axes::CaptureKind;
use super::block_type::block_type_for;
use super::capture_levels::measure_captures;
pub use super::install_error::InstallError;
use super::installed::{InstalledPlugin, IR_FOLDER, MANIFEST_FILE, NAM_FOLDER};
use super::manifest_build::{build_manifest, plugin_id, CaptureFile, PackageKind};
use super::models_pick::{file_name, unique_models};
use super::source_stamp::write_stamp;

/// What to install. `None` picks the default: A2 when the tone has A2
/// captures, and the block type its gear suggests.
#[derive(Debug, Clone, PartialEq)]
pub struct InstallRequest {
    pub tone_id: u64,
    pub architecture: Option<Tone3000Architecture>,
    pub block_type: Option<Tone3000BlockType>,
}

/// Where an install is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum InstallProgress {
    Fetching,
    Downloading { done: usize, total: usize },
    Measuring,
}

pub fn install_tone(
    api: &dyn Tone3000Api,
    root: &Path,
    request: &InstallRequest,
    progress: &mut dyn FnMut(InstallProgress),
) -> Result<InstalledPlugin, InstallError> {
    build_package(api, root, request, progress, false)
}

/// Downloads an installed tone again and swaps its package in place. The
/// old package stays until the new one is complete, and comes back if the
/// update fails.
pub fn update_tone(
    api: &dyn Tone3000Api,
    root: &Path,
    request: &InstallRequest,
    progress: &mut dyn FnMut(InstallProgress),
) -> Result<InstalledPlugin, InstallError> {
    build_package(api, root, request, progress, true)
}

fn build_package(
    api: &dyn Tone3000Api,
    root: &Path,
    request: &InstallRequest,
    progress: &mut dyn FnMut(InstallProgress),
    replace: bool,
) -> Result<InstalledPlugin, InstallError> {
    progress(InstallProgress::Fetching);
    let tone = api.tone(request.tone_id)?;
    let kind = package_kind(&tone, request.architecture);
    let id = plugin_id(tone.id, kind);
    let folder = root.join(match kind {
        PackageKind::Nam(_) => NAM_FOLDER,
        PackageKind::Ir => IR_FOLDER,
    });
    let dir = folder.join(&id);
    match (replace, dir.exists()) {
        (false, true) => return Err(InstallError::AlreadyInstalled(id)),
        (true, false) => return Err(InstallError::NotInstalled(id)),
        _ => {}
    }
    let architecture = match kind {
        PackageKind::Nam(arch) => Some(arch),
        PackageKind::Ir => None,
    };
    let models = unique_models(all_models(api, tone.id, architecture)?);
    if models.is_empty() {
        return Err(InstallError::NoCaptures);
    }
    let block_type = request.block_type.unwrap_or_else(|| block_type_for(&tone));

    std::fs::create_dir_all(&folder)?;
    let staging = folder.join(format!(".partial-{id}"));
    let previous = folder.join(format!(".previous-{id}"));
    let result = (|| {
        let _ = std::fs::remove_dir_all(&staging);
        let files = download_all(api, &staging, &models, kind, progress)?;
        progress(InstallProgress::Measuring);
        let capture_kind = match kind {
            PackageKind::Nam(_) => CaptureKind::Nam,
            PackageKind::Ir => CaptureKind::Ir,
        };
        let (files, output_gain_db) = measure_captures(&staging, files, capture_kind, block_type)?;
        let manifest = build_manifest(&tone, kind, block_type, &files, output_gain_db);
        plugin_loader::validate_manifest(&manifest)
            .map_err(|e| InstallError::Manifest(e.to_string()))?;
        let yaml =
            serde_yaml::to_string(&manifest).map_err(|e| InstallError::Manifest(e.to_string()))?;
        write_stamp(&staging, &tone, &files)?;
        if replace {
            let _ = std::fs::remove_dir_all(&previous);
            std::fs::rename(&dir, &previous)?;
        }
        std::fs::rename(&staging, &dir)?;
        let tmp = dir.join(format!(".{MANIFEST_FILE}.tmp"));
        std::fs::write(&tmp, yaml)?;
        std::fs::rename(&tmp, dir.join(MANIFEST_FILE))?;
        Ok(manifest)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    match result {
        Ok(manifest) => {
            let _ = std::fs::remove_dir_all(&previous);
            Ok(InstalledPlugin {
                plugin_id: id,
                dir,
                manifest,
                updated_at: tone.updated_at,
            })
        }
        Err(error) => {
            // A fresh install leaves nothing; an update puts the old
            // package back once it was moved aside.
            if !replace || previous.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            if replace && previous.exists() {
                let _ = std::fs::rename(&previous, &dir);
            }
            Err(error)
        }
    }
}

fn package_kind(tone: &Tone, architecture: Option<Tone3000Architecture>) -> PackageKind {
    if tone.format.as_deref() == Some("ir") {
        return PackageKind::Ir;
    }
    PackageKind::Nam(architecture.unwrap_or(if tone.a2_models_count > 0 {
        Tone3000Architecture::A2
    } else {
        Tone3000Architecture::A1
    }))
}

/// Downloads every capture as `captures/NNN.<ext>`.
fn download_all(
    api: &dyn Tone3000Api,
    staging: &Path,
    models: &[Model],
    kind: PackageKind,
    progress: &mut dyn FnMut(InstallProgress),
) -> Result<Vec<CaptureFile>, InstallError> {
    std::fs::create_dir_all(staging.join("captures"))?;
    let total = models.len();
    progress(InstallProgress::Downloading { done: 0, total });
    let mut files = Vec::with_capacity(total);
    for (i, model) in models.iter().enumerate() {
        let url = model.model_url.as_deref().unwrap_or_default();
        let file = PathBuf::from(format!("captures/{i:03}.{}", extension(url, kind)));
        api.download(url, &staging.join(&file))?;
        files.push(CaptureFile {
            name: model.name.clone(),
            file,
            output_gain_db: None,
        });
        progress(InstallProgress::Downloading { done: i + 1, total });
    }
    Ok(files)
}

fn extension(url: &str, kind: PackageKind) -> String {
    let name = file_name(url);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric()) => {
            ext.to_lowercase()
        }
        _ => match kind {
            PackageKind::Nam(_) => "nam".into(),
            PackageKind::Ir => "wav".into(),
        },
    }
}
