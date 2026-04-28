use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{
    normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, InsertBlock, InsertEndpoint, NamBlock, OutputBlock, OutputEntry, SelectBlock,
};
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{Chain, ChainInputMode, ChainOutputMixdown, ChainOutputMode};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fs;
use std::path::{Path, PathBuf};

mod block_yaml;
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

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainInputEntryYaml {
    #[serde(default, skip_serializing)]
    name: String,
    device_id: String,
    #[serde(default)]
    mode: ChainInputMode,
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainInputYaml {
    #[serde(default = "default_io_yaml_model")]
    model: String,
    // Legacy: name field migrated to entry-level
    #[serde(default, skip_serializing)]
    name: String,
    // New format: entries list
    #[serde(default)]
    entries: Vec<ChainInputEntryYaml>,
    // Legacy format: single device_id/mode/channels (for backward compat)
    #[serde(default, skip_serializing)]
    device_id: Option<String>,
    #[serde(default, skip_serializing)]
    mode: Option<ChainInputMode>,
    #[serde(default, skip_serializing)]
    channels: Option<Vec<usize>>,
}

pub(crate) fn default_io_yaml_model() -> String {
    "standard".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
pub(crate) struct ChainOutputEntryYaml {
    #[serde(default, skip_serializing)]
    name: String,
    device_id: String,
    #[serde(default)]
    mode: ChainOutputMode,
    channels: Vec<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainOutputYaml {
    #[serde(default = "default_io_yaml_model")]
    model: String,
    // Legacy: name field migrated to entry-level
    #[serde(default, skip_serializing)]
    name: String,
    // New format: entries list
    #[serde(default)]
    entries: Vec<ChainOutputEntryYaml>,
    // Legacy format: single device_id/mode/channels (for backward compat)
    #[serde(default, skip_serializing)]
    device_id: Option<String>,
    #[serde(default, skip_serializing)]
    mode: Option<ChainOutputMode>,
    #[serde(default, skip_serializing)]
    channels: Option<Vec<usize>>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(dead_code)]
struct ChainYaml {
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_instrument")]
    instrument: String,
    #[serde(default, skip_serializing)]
    enabled: bool,
    // Legacy multi-input/output fields — kept for backward-compatible deserialization, skipped on serialization
    #[serde(default, skip_serializing)]
    inputs: Vec<ChainInputYaml>,
    #[serde(default, skip_serializing)]
    outputs: Vec<ChainOutputYaml>,
    // Legacy fields — kept for backward-compatible deserialization, skipped on serialization
    #[serde(default, skip_serializing)]
    input_device_id: Option<String>,
    #[serde(default, skip_serializing)]
    input_channels: Option<Vec<usize>>,
    #[serde(default, skip_serializing)]
    output_device_id: Option<String>,
    #[serde(default, skip_serializing)]
    output_channels: Option<Vec<usize>>,
    #[serde(default)]
    blocks: Vec<Value>,
    #[serde(default, skip_serializing)]
    output_mixdown: ChainOutputMixdown,
    #[serde(default, skip_serializing)]
    input_mode: ChainInputMode,
}

impl ChainYaml {
    fn into_chain(self, index: usize) -> Result<Chain> {
        let chain_id = generated_chain_id(index);
        log::debug!("deserializing chain index={}, description={:?}, instrument='{}', enabled={}", index, self.description, self.instrument, self.enabled);

        // Parse all blocks from the blocks array (new format may include input/output blocks inline)
        let parsed_blocks: Vec<AudioBlock> = self
            .blocks
            .into_iter()
            .enumerate()
            .filter_map(|(block_index, block)| {
                load_audio_block_value(block, &chain_id, block_index)
            })
            .collect();

        // Check if blocks already contain Input/Output (new inline format)
        let has_inline_inputs = parsed_blocks.iter().any(|b| matches!(&b.kind, AudioBlockKind::Input(_)));
        let has_inline_outputs = parsed_blocks.iter().any(|b| matches!(&b.kind, AudioBlockKind::Output(_)));

        if has_inline_inputs || has_inline_outputs {
            // New format: blocks already contain I/O inline, use as-is
            let chain = Chain {
                id: chain_id.clone(),
                description: self.description,
                instrument: self.instrument,
                enabled: self.enabled,
                blocks: parsed_blocks,
            };
            return Ok(chain);
        }

        // Old format: convert separate inputs/outputs sections to blocks
        let mut input_blocks: Vec<AudioBlock> = self.inputs.into_iter().enumerate().map(|(i, inp)| {
            let entries = if !inp.entries.is_empty() {
                inp.entries.into_iter().map(|e| InputEntry {
                    device_id: DeviceId(e.device_id),
                    mode: e.mode,
                    channels: e.channels,
                }).collect()
            } else if let Some(device_id) = inp.device_id {
                vec![InputEntry {
                    device_id: DeviceId(device_id),
                    mode: inp.mode.unwrap_or_default(),
                    channels: inp.channels.unwrap_or_default(),
                }]
            } else {
                Vec::new()
            };
            AudioBlock {
                id: BlockId(format!("{}:input:{}", chain_id.0, i)),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: inp.model,
                    entries,
                }),
            }
        }).collect();

        let mut output_blocks: Vec<AudioBlock> = self.outputs.into_iter().enumerate().map(|(i, out)| {
            let entries = if !out.entries.is_empty() {
                out.entries.into_iter().map(|e| OutputEntry {
                    device_id: DeviceId(e.device_id),
                    mode: e.mode,
                    channels: e.channels,
                }).collect()
            } else if let Some(device_id) = out.device_id {
                vec![OutputEntry {
                    device_id: DeviceId(device_id),
                    mode: out.mode.unwrap_or_default(),
                    channels: out.channels.unwrap_or_default(),
                }]
            } else {
                Vec::new()
            };
            AudioBlock {
                id: BlockId(format!("{}:output:{}", chain_id.0, i)),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: out.model,
                    entries,
                }),
            }
        }).collect();

        // Oldest legacy format: single input_device_id/output_device_id fields
        if input_blocks.is_empty() {
            let legacy_device = self.input_device_id.unwrap_or_default();
            if !legacy_device.is_empty() {
                input_blocks.push(AudioBlock {
                    id: BlockId(format!("{}:input:0", chain_id.0)),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        entries: vec![InputEntry {
                            device_id: DeviceId(legacy_device),
                            mode: self.input_mode,
                            channels: self.input_channels.unwrap_or_default(),
                        }],
                    }),
                });
            }
        }
        if output_blocks.is_empty() {
            let legacy_device = self.output_device_id.unwrap_or_default();
            if !legacy_device.is_empty() {
                let legacy_channels = self.output_channels.unwrap_or_default();
                let mode = if legacy_channels.len() >= 2 {
                    ChainOutputMode::Stereo
                } else {
                    ChainOutputMode::Mono
                };
                output_blocks.push(AudioBlock {
                    id: BlockId(format!("{}:output:0", chain_id.0)),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        entries: vec![OutputEntry {
                            device_id: DeviceId(legacy_device),
                            mode,
                            channels: legacy_channels,
                        }],
                    }),
                });
            }
        }

        // Build blocks: inputs first, then audio blocks, then outputs
        let mut all_blocks = Vec::with_capacity(input_blocks.len() + parsed_blocks.len() + output_blocks.len());
        all_blocks.extend(input_blocks);
        all_blocks.extend(parsed_blocks);
        all_blocks.extend(output_blocks);

        let chain = Chain {
            id: chain_id.clone(),
            description: self.description,
            instrument: self.instrument,
            enabled: self.enabled,
            blocks: all_blocks,
        };

        Ok(chain)
    }

    fn from_chain(chain: &Chain) -> Result<Self> {
        // All blocks (including I/O) go into the blocks array
        let audio_blocks: Vec<Value> = chain
            .blocks
            .iter()
            .map(|block| {
                Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                    block,
                )?)?)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            description: chain.description.clone(),
            instrument: chain.instrument.clone(),
            enabled: false, // chains always start disabled on project load, regardless of saved state
            inputs: Vec::new(),
            outputs: Vec::new(),
            input_device_id: None,
            input_channels: None,
            output_device_id: None,
            output_channels: None,
            blocks: audio_blocks,
            output_mixdown: ChainOutputMixdown::Average,
            input_mode: ChainInputMode::default(),
        })
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

fn generated_chain_id(index: usize) -> ChainId {
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

fn default_instrument() -> String {
    block_core::DEFAULT_INSTRUMENT.to_string()
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
