use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, ChainId};
use domain::value_objects::ParameterValue;
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fs;
use std::path::{Path, PathBuf};

mod block_yaml;
mod chain_yaml;
use chain_yaml::ChainYaml;
use block_yaml::{AudioBlockYaml, load_audio_block_value};

pub struct YamlProjectRepository {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ChainBlocksPreset {
    pub id: String,
    pub name: Option<String>,
    pub blocks: Vec<project::block::AudioBlock>,
}

pub fn load_chain_preset_file(path: &Path) -> Result<ChainBlocksPreset> {
    log::info!("loading chain preset from {:?}", path);
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read preset yaml {:?}", path))?;
    let dto: PresetYaml = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse preset yaml {:?}", path))?;
    dto.into_preset()
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

pub fn serialize_project(project: &Project) -> Result<String> {
    let dto = ProjectYaml::from_project(project)?;
    Ok(serde_yaml::to_string(&dto)?)
}

pub fn serialize_audio_blocks(blocks: &[project::block::AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .map(|block| {
            Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                block,
            )?)?)
        })
        .collect()
}

impl YamlProjectRepository {
    pub fn load_current_project(&self) -> Result<Project> {
        log::info!("loading project from {:?}", self.path);
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        let dto: ProjectYaml = serde_yaml::from_str(&raw)?;
        let project = dto.into_project()?;
        log::debug!("project loaded: {} chains", project.chains.len());
        Ok(project)
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        log::info!("saving project to {:?}", self.path);
        let dto = ProjectYaml::from_project(project)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_yaml::to_string(&dto)?)?;
        log::debug!("project saved: {} chains", project.chains.len());
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, skip_serializing)]
    device_settings: Vec<DeviceSettingsYaml>,
    chains: Vec<ChainYaml>,
}

impl ProjectYaml {
    fn into_project(self) -> Result<Project> {
        Ok(Project {
            name: self.name,
            device_settings: self.device_settings.into_iter().map(Into::into).collect(),
            chains: self
                .chains
                .into_iter()
                .enumerate()
                .map(|(index, chain)| chain.into_chain(index))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn from_project(project: &Project) -> Result<Self> {
        Ok(Self {
            name: project.name.clone(),
            device_settings: Vec::new(),
            chains: project
                .chains
                .iter()
                .map(ChainYaml::from_chain)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PresetYaml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    blocks: Vec<Value>,
}

impl PresetYaml {
    fn into_preset(self) -> Result<ChainBlocksPreset> {
        let preset_chain_id = generated_preset_chain_id(&self.id);
        Ok(ChainBlocksPreset {
            id: self.id.clone(),
            name: self.name,
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
            id: preset.id.clone(),
            name: preset.name.clone(),
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

fn default_yaml_bit_depth() -> u32 {
    32
}

#[cfg(target_os = "linux")]
fn default_yaml_realtime() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn default_yaml_rt_priority() -> u8 {
    70
}

#[cfg(target_os = "linux")]
fn default_yaml_nperiods() -> u32 {
    3
}

#[derive(Debug, Deserialize, Serialize)]
struct DeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
    #[serde(default = "default_yaml_bit_depth")]
    bit_depth: u32,
    // Linux JACK tuning — only emitted on Linux. On macOS/Windows these
    // fields don't exist on DeviceSettings, so serialization skips them
    // and deserialization ignores them if present in a foreign YAML.
    #[cfg(target_os = "linux")]
    #[serde(default = "default_yaml_realtime")]
    realtime: bool,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_yaml_rt_priority")]
    rt_priority: u8,
    #[cfg(target_os = "linux")]
    #[serde(default = "default_yaml_nperiods")]
    nperiods: u32,
}

impl From<DeviceSettingsYaml> for DeviceSettings {
    fn from(value: DeviceSettingsYaml) -> Self {
        Self {
            device_id: DeviceId(value.device_id),
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
            bit_depth: value.bit_depth,
            #[cfg(target_os = "linux")]
            realtime: value.realtime,
            #[cfg(target_os = "linux")]
            rt_priority: value.rt_priority,
            #[cfg(target_os = "linux")]
            nperiods: value.nperiods,
        }
    }
}



pub(crate) fn flatten_parameter_set(value: Value) -> Result<ParameterSet> {
    let mut params = ParameterSet::default();
    match value {
        Value::Null => Ok(params),
        Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key = yaml_key_to_string(key)?;
                flatten_parameter_value(&mut params, &key, value)?;
            }
            Ok(params)
        }
        other => Err(anyhow!("params must be a mapping, got {:?}", other)),
    }
}

fn flatten_parameter_value(params: &mut ParameterSet, path: &str, value: Value) -> Result<()> {
    match value {
        Value::Mapping(mapping) => {
            for (key, nested_value) in mapping {
                let key = yaml_key_to_string(key)?;
                let nested_path = format!("{}.{}", path, key);
                flatten_parameter_value(params, &nested_path, nested_value)?;
            }
            Ok(())
        }
        scalar => {
            params.insert(path.to_string(), yaml_scalar_to_parameter_value(scalar)?);
            Ok(())
        }
    }
}

pub(crate) fn parameter_set_to_yaml_value(params: &ParameterSet) -> Value {
    let mut root = serde_yaml::Mapping::new();
    for (path, value) in &params.values {
        let parts = path.split('.').collect::<Vec<_>>();
        insert_yaml_value(&mut root, &parts, parameter_value_to_yaml(value));
    }
    Value::Mapping(root)
}

fn insert_yaml_value(mapping: &mut serde_yaml::Mapping, path: &[&str], value: Value) {
    if path.is_empty() {
        return;
    }
    let key = Value::String(path[0].to_string());
    if path.len() == 1 {
        mapping.insert(key, value);
        return;
    }

    if !matches!(mapping.get(&key), Some(Value::Mapping(_))) {
        mapping.insert(key.clone(), Value::Mapping(serde_yaml::Mapping::new()));
    }

    if let Some(Value::Mapping(child)) = mapping.get_mut(&key) {
        insert_yaml_value(child, &path[1..], value);
    }
}

fn parameter_value_to_yaml(value: &ParameterValue) -> Value {
    match value {
        ParameterValue::Null => Value::Null,
        ParameterValue::Bool(value) => Value::Bool(*value),
        ParameterValue::Int(value) => serde_yaml::to_value(value).unwrap_or(Value::Null),
        ParameterValue::Float(value) => serde_yaml::to_value(value).unwrap_or(Value::Null),
        ParameterValue::String(value) => Value::String(value.clone()),
    }
}

fn yaml_key_to_string(value: Value) -> Result<String> {
    match value {
        Value::String(value) => Ok(value),
        other => Err(anyhow!("yaml object keys must be strings, got {:?}", other)),
    }
}

pub(crate) fn yaml_scalar_to_parameter_value(value: Value) -> Result<ParameterValue> {
    match value {
        Value::Null => Ok(ParameterValue::Null),
        Value::Bool(value) => Ok(ParameterValue::Bool(value)),
        Value::Number(value) => {
            if let Some(number) = value.as_i64() {
                Ok(ParameterValue::Int(number))
            } else if let Some(number) = value.as_f64() {
                Ok(ParameterValue::Float(number as f32))
            } else {
                Err(anyhow!("unsupported yaml number '{}'", value))
            }
        }
        Value::String(value) => Ok(ParameterValue::String(value)),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => {
            Err(anyhow!("unsupported yaml value in params"))
        }
    }
}

pub(crate) fn generated_block_id(chain_id: &ChainId, index: usize) -> BlockId {
    BlockId(format!("{}:block:{}", chain_id.0, index))
}

pub(crate) fn generated_chain_id(index: usize) -> ChainId {
    ChainId(format!("chain:{}", index))
}

fn generated_preset_chain_id(preset_id: &str) -> ChainId {
    ChainId(format!("preset:{}", preset_id))
}

pub(crate) fn default_delay_model() -> String {
    block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model")
        .to_string()
}

pub(crate) fn default_nam_model() -> String {
    block_nam::supported_models()
        .first()
        .expect("block-nam must expose at least one model")
        .to_string()
}

pub(crate) fn default_preamp_model() -> String {
    block_preamp::supported_models()
        .first()
        .expect("block-preamp must expose at least one model")
        .to_string()
}

pub(crate) fn default_amp_model() -> String {
    block_amp::supported_models()
        .first()
        .expect("block-amp must expose at least one model")
        .to_string()
}

pub(crate) fn default_full_rig_model() -> String {
    block_full_rig::supported_models()
        .first()
        .expect("block-full-rig must expose at least one model")
        .to_string()
}

pub(crate) fn default_cab_model() -> String {
    block_cab::supported_models()
        .first()
        .expect("block-cab must expose at least one model")
        .to_string()
}

pub(crate) fn default_body_model() -> String {
    block_body::supported_models()
        .first()
        .expect("block-body must expose at least one model")
        .to_string()
}

pub(crate) fn default_drive_model() -> String {
    block_gain::supported_models()
        .first()
        .expect("block-gain must expose at least one model")
        .to_string()
}

pub(crate) fn default_reverb_model() -> String {
    block_reverb::supported_models()
        .first()
        .expect("block-reverb must expose at least one model")
        .to_string()
}

pub(crate) fn default_utility_model() -> String {
    block_util::supported_models()
        .first()
        .expect("block-util must expose at least one model")
        .to_string()
}

pub(crate) fn default_dynamics_model() -> String {
    block_dyn::supported_models()
        .first()
        .expect("block-dyn must expose at least one model")
        .to_string()
}

pub(crate) fn default_filter_model() -> String {
    block_filter::supported_models()
        .first()
        .expect("block-filter must expose at least one model")
        .to_string()
}

pub(crate) fn default_ir_model() -> String {
    block_ir::supported_models()
        .first()
        .expect("block-ir must expose at least one model")
        .to_string()
}

pub(crate) fn default_wah_model() -> String {
    block_wah::supported_models()
        .first()
        .expect("block-wah must expose at least one model")
        .to_string()
}

pub(crate) fn default_modulation_model() -> String {
    block_mod::supported_models()
        .first()
        .expect("block-mod must expose at least one model")
        .to_string()
}

pub(crate) fn default_pitch_model() -> String {
    block_pitch::supported_models()
        .first()
        .expect("block-pitch must expose at least one model")
        .to_string()
}

pub(crate) const fn default_enabled() -> bool {
    true
}

pub(crate) fn default_instrument() -> String {
    block_core::DEFAULT_INSTRUMENT.to_string()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
