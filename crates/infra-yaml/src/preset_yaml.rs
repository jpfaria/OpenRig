//! Responsibility: maps a chain preset file onto the blocks it carries.
//!
//! Split out of `lib.rs` (#873).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fs;
use std::path::Path;

use crate::block_yaml::AudioBlockYaml;
use crate::block_yaml_load::load_audio_block_value;
use crate::generated_preset_chain_id;

#[derive(Debug, Clone)]
pub struct ChainBlocksPreset {
    pub id: String,
    pub name: Option<String>,
    /// Output volume do preset em percentual. 100 = unity. Default ao
    /// carregar quando o campo `volume:` está ausente do YAML.
    pub volume: f32,
    /// Instrument this preset was saved for. Defaults to "electric_guitar"
    /// for back-compat with untagged legacy preset files.
    pub instrument: String,
    pub blocks: Vec<project::block::AudioBlock>,
}

pub fn load_chain_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    log::info!("loading chain preset from {:?}", path);
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read preset yaml {:?}", path))?;
    let dto: PresetYaml = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse preset yaml {:?}", path))?;
    if dto.version > project::rig::PRESET_FORMAT_VERSION {
        anyhow::bail!(
            "preset {:?} version {} is newer than this build supports (max {}); \
             please upgrade OpenRig",
            path,
            dto.version,
            project::rig::PRESET_FORMAT_VERSION
        );
    }
    dto.into_preset()
}

/// Load a legacy standalone preset file and convert it into a [`RigPreset`]
/// (the missing "+ presets" half of #450). Returns the human name (falling
/// back to the preset id) and the converted preset. Blocks and volume are
/// preserved exact, so audio is identical to the legacy preset.
pub fn load_legacy_preset_as_rig(path: &Path) -> Result<(String, project::rig::RigPreset)> {
    let legacy = load_chain_preset_file(path)?;
    let name = legacy
        .name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| legacy.id.clone());
    let rig = project::rig::RigPreset::from_legacy_blocks(legacy.blocks, legacy.volume);
    Ok((name, rig))
}

pub fn save_chain_preset_file(path: &Path, preset: &ChainBlocksPreset) -> Result<()> {
    log::info!("saving chain preset to {:?}", path);
    let dto = PresetYaml::from_chain_preset(preset)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_yaml::to_string(&dto)?)?;
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PresetYaml {
    /// Missing ⇒ a pre-version preset, whose shape *is* v1.
    #[serde(default = "default_preset_doc_version")]
    version: u32,
    id: String,
    #[serde(default)]
    name: Option<String>,
    /// Output volume do preset em percentual. Default 100 (unity) quando
    /// ausente do YAML. Multiplicado no master output do engine.
    #[serde(default = "default_preset_volume")]
    volume: f32,
    /// Instrument tag added in #627. Missing in legacy files ⇒ defaults to
    /// "electric_guitar" for back-compat.
    #[serde(default = "default_preset_instrument")]
    instrument: String,
    #[serde(default)]
    blocks: Vec<Value>,
}

fn default_preset_volume() -> f32 {
    100.0
}

fn default_preset_doc_version() -> u32 {
    1
}

fn default_preset_instrument() -> String {
    block_core::INST_ELECTRIC_GUITAR.to_string()
}

impl PresetYaml {
    fn into_preset(self) -> Result<ChainBlocksPreset> {
        let preset_chain_id = generated_preset_chain_id(&self.id);
        Ok(ChainBlocksPreset {
            id: self.id.clone(),
            name: self.name,
            volume: self.volume,
            instrument: self.instrument,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .filter_map(|(index, block)| load_audio_block_value(block, &preset_chain_id, index))
                .collect(),
        })
    }

    fn from_chain_preset(preset: &ChainBlocksPreset) -> Result<Self> {
        Ok(Self {
            version: project::rig::PRESET_FORMAT_VERSION,
            id: preset.id.clone(),
            name: preset.name.clone(),
            volume: preset.volume,
            instrument: preset.instrument.clone(),
            blocks: preset
                .blocks
                .iter()
                .map(|block| {
                    Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                        block,
                    )?)?)
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}
