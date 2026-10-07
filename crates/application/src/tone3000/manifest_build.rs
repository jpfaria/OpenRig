//! Responsibility: writes the plugin manifest of a downloaded TONE3000 tone.

use std::path::PathBuf;

use plugin_loader::manifest::{Backend, GridCapture, NamArchitecture, PluginManifest};

use super::api_enums::{Tone3000Architecture, Tone3000BlockType};
use super::api_types::Tone;
use super::axes::{infer_axes, CaptureKind};

const TONE_PAGE: &str = "https://www.tone3000.com/tones";

/// What a package holds: NAM captures of one architecture, or IRs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    Nam(Tone3000Architecture),
    Ir,
}

/// One downloaded capture: its TONE3000 name, its path inside the package
/// and, for IRs, its own level.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureFile {
    pub name: String,
    pub file: PathBuf,
    pub output_gain_db: Option<f32>,
}

/// `tone3000_<id>_a1|a2` for NAM, `tone3000_<id>` for IR. One tone can be
/// installed once per architecture.
pub fn plugin_id(tone_id: u64, kind: PackageKind) -> String {
    match kind {
        PackageKind::Nam(arch) => format!("tone3000_{tone_id}_{}", arch.as_id_suffix()),
        PackageKind::Ir => format!("tone3000_{tone_id}"),
    }
}

pub fn build_manifest(
    tone: &Tone,
    kind: PackageKind,
    block_type: Tone3000BlockType,
    files: &[CaptureFile],
    output_gain_db: Option<f32>,
) -> PluginManifest {
    let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
    let capture_kind = match kind {
        PackageKind::Nam(_) => CaptureKind::Nam,
        PackageKind::Ir => CaptureKind::Ir,
    };
    let axes = infer_axes(&names, capture_kind);
    let captures: Vec<GridCapture> = files
        .iter()
        .zip(axes.values)
        .map(|(file, values)| GridCapture {
            values,
            file: file.file.clone(),
            output_gain_db: file.output_gain_db,
            noise_gate: None,
        })
        .collect();
    let parameters = axes.parameters;
    let (backend, architecture) = match kind {
        PackageKind::Nam(arch) => (
            Backend::Nam {
                parameters,
                captures,
            },
            Some(match arch {
                Tone3000Architecture::A1 => NamArchitecture::A1,
                Tone3000Architecture::A2 => NamArchitecture::A2,
            }),
        ),
        PackageKind::Ir => (
            Backend::Ir {
                parameters,
                captures,
            },
            None,
        ),
    };
    let page = tone
        .url
        .clone()
        .unwrap_or_else(|| format!("{TONE_PAGE}/{}", tone.id));
    PluginManifest {
        manifest_version: 1,
        id: plugin_id(tone.id, kind),
        display_name: tone.title.clone(),
        author: tone
            .user
            .as_ref()
            .map(|u| u.display_name.clone().unwrap_or_else(|| u.username.clone())),
        description: tone.description.clone(),
        inspired_by: tone.makes.first().map(|m| m.name.clone()),
        brand: brand(tone),
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: tone.license.clone(),
        homepage: tone.url.clone(),
        sources: Some(vec![page]),
        output_gain_db,
        noise_gate: None,
        architecture,
        block_type: block_type.manifest_block_type(),
        backend,
    }
}

/// The brand of the first make whose brand word the title names, lowercase.
/// A make's brand word is its first word that is not a number (a year).
fn brand(tone: &Tone) -> Option<String> {
    let title: Vec<String> = tone
        .title
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .collect();
    tone.makes.iter().find_map(|make| {
        let word = make
            .name
            .split_whitespace()
            .find(|w| !w.chars().all(|c| c.is_ascii_digit()))?
            .to_lowercase();
        title.contains(&word).then_some(word)
    })
}
