//! Responsibility: builds a new capture plugin in the plugins folder.
//!
//! The captures are copied into a hidden staging folder that the loader
//! never reads as a package, levelled like a TONE3000 install, and the
//! folder is renamed into place only when its manifest is complete. The
//! source files stay where they were.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use plugin_loader::manifest::{Backend, GridCapture, PluginManifest};
use project::block::capture_engine_params::capture_engine_parameter_names;

use super::grid_apply::grid_parameters;
use super::grid_types::{CaptureBackend, EditorGrid};
use super::manifest_save::write_manifest;
use crate::tone3000::axes::CaptureKind;
use crate::tone3000::capture_levels::measure_captures;
use crate::tone3000::installed::is_plugin_id;
use crate::tone3000::manifest_build::CaptureFile;
use crate::tone3000::Tone3000BlockType;

/// What the user asked for.
#[derive(Debug, Clone)]
pub struct CreateRequest {
    pub display_name: String,
    pub brand: Option<String>,
    pub block_type: Tone3000BlockType,
    pub backend: CaptureBackend,
    pub grid: EditorGrid,
}

/// Refuses a request that can never build, before any file is touched.
pub fn check_request(request: &CreateRequest) -> Result<()> {
    if request.display_name.trim().is_empty() {
        bail!("a new plugin needs a name");
    }
    if request.grid.rows.is_empty() {
        bail!("a new plugin needs at least one capture");
    }
    grid_parameters(&request.grid, &capture_engine_parameter_names())?;
    Ok(())
}

/// Builds the plugin under `<plugins_folder>/<nam|ir>/<id>/`, loads it into
/// the catalog and returns its id.
pub fn create_plugin(plugins_folder: &Path, request: &CreateRequest) -> Result<String> {
    check_request(request)?;
    let (parameters, values) = grid_parameters(&request.grid, &capture_engine_parameter_names())?;
    let (kind, folder) = match request.backend {
        CaptureBackend::Nam => (CaptureKind::Nam, "nam"),
        CaptureBackend::Ir => (CaptureKind::Ir, "ir"),
    };
    let parent = plugins_folder.join(folder);
    std::fs::create_dir_all(&parent)?;
    let id = free_id(&parent, &request.display_name);
    let staging = parent.join(format!(".partial-{id}"));
    let result = (|| {
        let _ = std::fs::remove_dir_all(&staging);
        let files = copy_captures(&staging, &request.grid, kind)?;
        let (files, output_gain_db) = measure_captures(&staging, files, kind, request.block_type)
            .map_err(|e| anyhow!("{e}"))?;
        let captures: Vec<GridCapture> = files
            .into_iter()
            .zip(values)
            .map(|(file, values)| GridCapture {
                values,
                file: file.file,
                output_gain_db: file.output_gain_db,
                noise_gate: None,
            })
            .collect();
        let manifest = manifest(&id, request, kind, parameters, captures, output_gain_db);
        plugin_loader::validate_manifest(&manifest).map_err(|e| anyhow!("{e}"))?;
        write_manifest(&staging, &manifest)?;
        std::fs::rename(&staging, parent.join(&id))?;
        Ok::<_, anyhow::Error>(())
    })();
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    plugin_loader::registry::load_one(&id, &[parent])
        .map_err(|e| anyhow!("created, but the catalog did not load it: {e}"))?;
    Ok(id)
}

/// Copies every row's file as `captures/NNN.<ext>`.
fn copy_captures(staging: &Path, grid: &EditorGrid, kind: CaptureKind) -> Result<Vec<CaptureFile>> {
    std::fs::create_dir_all(staging.join("captures"))?;
    grid.rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            if !row.file.is_file() {
                bail!("{} is not a file", row.file.display());
            }
            let file = PathBuf::from(format!("captures/{i:03}.{}", extension(&row.file, kind)));
            std::fs::copy(&row.file, staging.join(&file))
                .with_context(|| format!("copying {}", row.file.display()))?;
            Ok(CaptureFile {
                name: row
                    .file
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                file,
                output_gain_db: None,
            })
        })
        .collect()
}

fn extension(file: &Path, kind: CaptureKind) -> String {
    match file.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric()) => {
            ext.to_lowercase()
        }
        _ => match kind {
            CaptureKind::Nam => "nam".into(),
            CaptureKind::Ir => "wav".into(),
        },
    }
}

/// An id made from the name, unused by the catalog and by `parent`.
fn free_id(parent: &Path, display_name: &str) -> String {
    let mut base: String = display_name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    base = base
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if base.is_empty() || is_plugin_id(&base) {
        base = format!("plugin_{base}").trim_end_matches('_').to_string();
    }
    let taken = |id: &str| {
        plugin_loader::registry::find(id).is_some()
            || parent.join(id).exists()
            || parent.join(format!(".partial-{id}")).exists()
    };
    let mut id = base.clone();
    let mut n = 2;
    while taken(&id) {
        id = format!("{base}_{n}");
        n += 1;
    }
    id
}

fn manifest(
    id: &str,
    request: &CreateRequest,
    kind: CaptureKind,
    parameters: Vec<plugin_loader::manifest::GridParameter>,
    captures: Vec<GridCapture>,
    output_gain_db: Option<f32>,
) -> PluginManifest {
    let backend = match kind {
        CaptureKind::Nam => Backend::Nam {
            parameters,
            captures,
        },
        CaptureKind::Ir => Backend::Ir {
            parameters,
            captures,
        },
    };
    PluginManifest {
        manifest_version: 1,
        id: id.to_string(),
        display_name: request.display_name.trim().to_string(),
        author: None,
        description: None,
        inspired_by: None,
        brand: request
            .brand
            .as_deref()
            .map(str::trim)
            .filter(|b| !b.is_empty())
            .map(str::to_string),
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        sources: None,
        output_gain_db,
        noise_gate: None,
        architecture: None,
        block_type: request.block_type.manifest_block_type(),
        backend,
    }
}
