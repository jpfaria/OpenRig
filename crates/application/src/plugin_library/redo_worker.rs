//! Responsibility: asks TONE3000 for what a redo of an installed tone needs.
//!
//! Runs on a worker thread. A tone that changed on TONE3000 is downloaded
//! again, keeping the package's saved versions; otherwise only the current
//! capture names are fetched, and the dispatcher rebuilds the parameters.

use std::path::{Path, PathBuf};

use plugin_loader::manifest::{BlockType, GridCapture, NamArchitecture, PluginManifest};
use plugin_loader::version_store::{read_version, version_numbers, write_version};

use super::capture_names::capture_names;
use super::disk_manifest::read_disk_manifest;
use super::manifest_save::save_manifest_version;
use crate::tone3000::api_client::{all_models, Tone3000Api};
use crate::tone3000::install::{update_tone, InstallRequest};
use crate::tone3000::models_pick::unique_models;
use crate::tone3000::source_stamp::read_updated_at;
use crate::tone3000::{Tone3000Architecture, Tone3000BlockType};

/// What a redo worker reports back.
#[derive(Debug)]
pub enum PluginLibraryDone {
    /// The tone is current: rebuild the parameters from these names, one
    /// per capture of the manifest on disk.
    Names {
        plugin_id: String,
        names: Vec<String>,
    },
    /// The tone was downloaded again and saved as `version`.
    Updated {
        plugin_id: String,
        from: Box<PluginManifest>,
        to: Box<PluginManifest>,
        version: u32,
    },
    Failed {
        plugin_id: String,
        message: String,
    },
}

/// The installed tone a redo works on.
pub struct RedoTone {
    pub plugin_id: String,
    pub tone_id: u64,
    /// The TONE3000 install folder.
    pub tone3000_root: PathBuf,
    /// The package folder.
    pub package_root: PathBuf,
    pub manifest: PluginManifest,
}

pub fn redo_tone(api: &dyn Tone3000Api, job: RedoTone) -> PluginLibraryDone {
    let plugin_id = job.plugin_id.clone();
    run(api, job).unwrap_or_else(|message| PluginLibraryDone::Failed { plugin_id, message })
}

fn run(api: &dyn Tone3000Api, job: RedoTone) -> Result<PluginLibraryDone, String> {
    let tone = api.tone(job.tone_id).map_err(|e| e.to_string())?;
    let architecture = job.manifest.architecture.map(|a| match a {
        NamArchitecture::A1 => Tone3000Architecture::A1,
        NamArchitecture::A2 => Tone3000Architecture::A2,
    });
    if is_newer(
        tone.updated_at.as_deref(),
        read_updated_at(&job.package_root).as_deref(),
    ) {
        return update(api, job, architecture);
    }
    let models =
        unique_models(all_models(api, job.tone_id, architecture).map_err(|e| e.to_string())?);
    let captures: &[GridCapture] = project::block::grid_version_follow::capture_grid(&job.manifest)
        .map(|(_, captures)| captures)
        .ok_or_else(|| "not a capture plugin".to_string())?;
    let recorded = capture_names(&job.package_root, captures);
    let names = captures
        .iter()
        .zip(recorded)
        .map(|(capture, recorded)| {
            download_index(&capture.file)
                .and_then(|i| models.get(i))
                .map(|m| m.name.clone())
                .unwrap_or(recorded)
        })
        .collect();
    Ok(PluginLibraryDone::Names {
        plugin_id: job.plugin_id,
        names,
    })
}

/// Downloads the tone again and keeps the versions saved before it.
fn update(
    api: &dyn Tone3000Api,
    job: RedoTone,
    architecture: Option<Tone3000Architecture>,
) -> Result<PluginLibraryDone, String> {
    let kept: Vec<(u32, PluginManifest)> = version_numbers(&job.package_root)
        .into_iter()
        .filter_map(|n| read_version(&job.package_root, n).map(|m| (n, m)))
        .collect();
    let request = InstallRequest {
        tone_id: job.tone_id,
        architecture,
        block_type: tone3000_block_type(job.manifest.block_type),
    };
    update_tone(api, &job.tone3000_root, &request, &mut |_| {}).map_err(|e| e.to_string())?;
    for (n, manifest) in &kept {
        write_version(&job.package_root, *n, manifest).map_err(|e| e.to_string())?;
    }
    let next = read_disk_manifest(&job.package_root).map_err(|e| format!("{e:#}"))?;
    let version = save_manifest_version(&job.package_root, &job.manifest, &next)
        .map_err(|e| format!("{e:#}"))?;
    Ok(PluginLibraryDone::Updated {
        plugin_id: job.plugin_id,
        from: Box::new(job.manifest),
        to: Box::new(next),
        version,
    })
}

/// A newer tone on TONE3000 than the one installed (timestamps in the
/// same ISO 8601 form compare as text). An unknown installed version is
/// treated as old.
fn is_newer(remote: Option<&str>, installed: Option<&str>) -> bool {
    match (remote, installed) {
        (Some(remote), Some(installed)) => remote > installed,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// An install saves the capture of model `i` as `captures/<i>.<ext>`.
fn download_index(file: &Path) -> Option<usize> {
    file.file_stem()?.to_str()?.parse().ok()
}

fn tone3000_block_type(block_type: BlockType) -> Option<Tone3000BlockType> {
    [
        Tone3000BlockType::Amp,
        Tone3000BlockType::Preamp,
        Tone3000BlockType::GainPedal,
        Tone3000BlockType::Cab,
        Tone3000BlockType::Body,
    ]
    .into_iter()
    .find(|t| t.manifest_block_type() == block_type)
}
