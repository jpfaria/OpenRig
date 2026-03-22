use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{
    normalize_block_params, AudioBlock, AudioBlockKind, CoreBlock, NamBlock, SelectBlock,
};
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use project::chain::{Chain, ChainOutputMixdown};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::fs;
use std::path::{Path, PathBuf};

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
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read preset yaml {:?}", path))?;
    let dto: PresetYaml = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse preset yaml {:?}", path))?;
    dto.into_preset()
}

pub fn save_chain_preset_file(path: &Path, preset: &ChainBlocksPreset) -> Result<()> {
    let dto = PresetYaml::from_chain_preset(preset)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_yaml::to_string(&dto)?)?;
    Ok(())
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
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        let dto: ProjectYaml = serde_yaml::from_str(&raw)?;
        dto.into_project()
    }

    pub fn save_project(&self, project: &Project) -> Result<()> {
        let dto = ProjectYaml::from_project(project)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_yaml::to_string(&dto)?)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
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
            device_settings: project
                .device_settings
                .iter()
                .map(DeviceSettingsYaml::from_settings)
                .collect(),
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

#[derive(Debug, Deserialize, Serialize)]
struct DeviceSettingsYaml {
    device_id: String,
    sample_rate: u32,
    buffer_size_frames: u32,
}

impl From<DeviceSettingsYaml> for DeviceSettings {
    fn from(value: DeviceSettingsYaml) -> Self {
        Self {
            device_id: DeviceId(value.device_id),
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
        }
    }
}

impl DeviceSettingsYaml {
    fn from_settings(settings: &DeviceSettings) -> Self {
        Self {
            device_id: settings.device_id.0.clone(),
            sample_rate: settings.sample_rate,
            buffer_size_frames: settings.buffer_size_frames,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ChainYaml {
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_enabled", skip_serializing)]
    enabled: bool,
    input_device_id: String,
    input_channels: Vec<usize>,
    output_device_id: String,
    output_channels: Vec<usize>,
    #[serde(default)]
    blocks: Vec<Value>,
    #[serde(default)]
    output_mixdown: ChainOutputMixdown,
}

impl ChainYaml {
    fn into_chain(self, index: usize) -> Result<Chain> {
        let chain_id = generated_chain_id(index);
        Ok(Chain {
            id: chain_id.clone(),
            description: self.description,
            enabled: false, // chains always start disabled on load
            input_device_id: DeviceId(self.input_device_id),
            input_channels: self.input_channels,
            output_device_id: DeviceId(self.output_device_id),
            output_channels: self.output_channels,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .filter_map(|(block_index, block)| {
                    load_audio_block_value(block, &chain_id, block_index)
                })
                .collect(),
            output_mixdown: self.output_mixdown,
        })
    }

    fn from_chain(chain: &Chain) -> Result<Self> {
        Ok(Self {
            description: chain.description.clone(),
            enabled: chain.enabled,
            input_device_id: chain.input_device_id.0.clone(),
            input_channels: chain.input_channels.clone(),
            output_device_id: chain.output_device_id.0.clone(),
            output_channels: chain.output_channels.clone(),
            blocks: chain
                .blocks
                .iter()
                .map(|block| {
                    Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(
                        block,
                    )?)?)
                })
                .collect::<Result<Vec<_>>>()?,
            output_mixdown: chain.output_mixdown,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AudioBlockYaml {
    #[serde(rename = "preamp")]
    Preamp {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_preamp_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "amp")]
    Amp {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_amp_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "full_rig")]
    FullRig {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_full_rig_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Cab {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_cab_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Ir {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_ir_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "gain")]
    Gain {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_drive_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Nam {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_nam_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Delay {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_delay_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Reverb {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_reverb_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Utility {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_utility_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Dynamics {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_dynamics_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Filter {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_filter_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Wah {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_wah_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Modulation {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_modulation_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Pitch {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_pitch_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Select {
        #[serde(default = "default_enabled")]
        enabled: bool,
        selected: String,
        options: Vec<SelectOptionYaml>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
struct SelectOptionYaml {
    id: String,
    #[serde(flatten)]
    block: AudioBlockYaml,
}

impl AudioBlockYaml {
    fn into_audio_block(self, chain_id: &ChainId, index: usize) -> Result<AudioBlock> {
        self.into_audio_block_with_id(generated_block_id(chain_id, index))
    }

    fn into_audio_block_with_id(self, generated_id: BlockId) -> Result<AudioBlock> {
        match self {
            AudioBlockYaml::Nam {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Nam(NamBlock {
                    model: model.clone(),
                    params: load_model_params("nam", &model, params)?,
                }),
            }),
            AudioBlockYaml::Select {
                enabled,
                selected,
                options,
            } => {
                let select_prefix = generated_id.0.clone();
                let selected_block_id = BlockId(format!("{}::{}", select_prefix, selected));
                let options = options
                    .into_iter()
                    .map(|option| {
                        let option_id = BlockId(format!("{}::{}", select_prefix, option.id));
                        option.block.into_audio_block_with_id(option_id)
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(AudioBlock {
                    id: generated_id,
                    enabled,
                    kind: AudioBlockKind::Select(SelectBlock {
                        selected_block_id,
                        options,
                    }),
                })
            }
            other => {
                let (effect_type, enabled, model, params) = extract_core_block_fields(other);
                Ok(AudioBlock {
                    id: generated_id,
                    enabled,
                    kind: AudioBlockKind::Core(CoreBlock {
                        effect_type: effect_type.to_string(),
                        model: model.clone(),
                        params: load_model_params(effect_type, &model, params)?,
                    }),
                })
            }
        }
    }

    fn from_audio_block(block: &AudioBlock) -> Result<Self> {
        match &block.kind {
            AudioBlockKind::Nam(stage) => Ok(Self::Nam {
                enabled: block.enabled,
                model: stage.model.clone(),
                params: parameter_set_to_yaml_value(&stage.params),
            }),
            AudioBlockKind::Core(core) => {
                let params = parameter_set_to_yaml_value(&core.params);
                let enabled = block.enabled;
                let model = core.model.clone();
                match core.effect_type.as_str() {
                    "preamp" => Ok(Self::Preamp { enabled, model, params }),
                    "amp" => Ok(Self::Amp { enabled, model, params }),
                    "full_rig" => Ok(Self::FullRig { enabled, model, params }),
                    "cab" => Ok(Self::Cab { enabled, model, params }),
                    "ir" => Ok(Self::Ir { enabled, model, params }),
                    "gain" => Ok(Self::Gain { enabled, model, params }),
                    "delay" => Ok(Self::Delay { enabled, model, params }),
                    "reverb" => Ok(Self::Reverb { enabled, model, params }),
                    "utility" => Ok(Self::Utility { enabled, model, params }),
                    "dynamics" => Ok(Self::Dynamics { enabled, model, params }),
                    "filter" => Ok(Self::Filter { enabled, model, params }),
                    "wah" => Ok(Self::Wah { enabled, model, params }),
                    "modulation" => Ok(Self::Modulation { enabled, model, params }),
                    "pitch" => Ok(Self::Pitch { enabled, model, params }),
                    other => Err(anyhow!("unsupported core block effect_type '{}'", other)),
                }
            }
            AudioBlockKind::Select(select) => {
                let selected = select
                    .selected_block_id
                    .0
                    .strip_prefix(&format!("{}::", block.id.0))
                    .unwrap_or(select.selected_block_id.0.as_str())
                    .to_string();
                let options = select
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        Ok(SelectOptionYaml {
                            id: option
                                .id
                                .0
                                .strip_prefix(&format!("{}::", block.id.0))
                                .unwrap_or(option.id.0.as_str())
                                .to_string(),
                            block: AudioBlockYaml::from_audio_block(option)
                                .with_context(|| {
                                    format!(
                                        "failed to serialize select option {} for block '{}'",
                                        index,
                                        block.id.0
                                    )
                                })?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(Self::Select {
                    enabled: block.enabled,
                    selected,
                    options,
                })
            }
        }
    }
}

fn load_audio_block_value(value: Value, chain_id: &ChainId, index: usize) -> Option<AudioBlock> {
    let yaml = match serde_yaml::from_value::<AudioBlockYaml>(value) {
        Ok(yaml) => yaml,
        Err(error) => {
            eprintln!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            return None;
        }
    };

    match yaml.into_audio_block(chain_id, index) {
        Ok(block) => Some(block),
        Err(error) => {
            eprintln!(
                "ignoring unsupported or invalid block at {}:{}: {}",
                chain_id.0, index, error
            );
            None
        }
    }
}

fn load_model_params(effect_type: &str, model: &str, raw_params: Value) -> Result<ParameterSet> {
    let flattened = flatten_parameter_set(raw_params)?;
    normalize_block_params(effect_type, model, flattened).map_err(anyhow::Error::msg)
}

fn extract_core_block_fields(yaml: AudioBlockYaml) -> (&'static str, bool, String, Value) {
    match yaml {
        AudioBlockYaml::Preamp { enabled, model, params } => ("preamp", enabled, model, params),
        AudioBlockYaml::Amp { enabled, model, params } => ("amp", enabled, model, params),
        AudioBlockYaml::FullRig { enabled, model, params } => ("full_rig", enabled, model, params),
        AudioBlockYaml::Cab { enabled, model, params } => ("cab", enabled, model, params),
        AudioBlockYaml::Ir { enabled, model, params } => ("ir", enabled, model, params),
        AudioBlockYaml::Gain { enabled, model, params } => ("gain", enabled, model, params),
        AudioBlockYaml::Delay { enabled, model, params } => ("delay", enabled, model, params),
        AudioBlockYaml::Reverb { enabled, model, params } => ("reverb", enabled, model, params),
        AudioBlockYaml::Utility { enabled, model, params } => ("utility", enabled, model, params),
        AudioBlockYaml::Dynamics { enabled, model, params } => ("dynamics", enabled, model, params),
        AudioBlockYaml::Filter { enabled, model, params } => ("filter", enabled, model, params),
        AudioBlockYaml::Wah { enabled, model, params } => ("wah", enabled, model, params),
        AudioBlockYaml::Modulation { enabled, model, params } => ("modulation", enabled, model, params),
        AudioBlockYaml::Pitch { enabled, model, params } => ("pitch", enabled, model, params),
        // Nam and Select are handled separately, should never reach here
        AudioBlockYaml::Nam { enabled, model, params } => ("nam", enabled, model, params),
        AudioBlockYaml::Select { .. } => unreachable!("Select handled before extract_core_block_fields"),
    }
}

fn flatten_parameter_set(value: Value) -> Result<ParameterSet> {
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

fn parameter_set_to_yaml_value(params: &ParameterSet) -> Value {
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

fn yaml_scalar_to_parameter_value(value: Value) -> Result<ParameterValue> {
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

fn generated_block_id(chain_id: &ChainId, index: usize) -> BlockId {
    BlockId(format!("{}:block:{}", chain_id.0, index))
}

fn generated_chain_id(index: usize) -> ChainId {
    ChainId(format!("chain:{}", index))
}

fn generated_preset_chain_id(preset_id: &str) -> ChainId {
    ChainId(format!("preset:{}", preset_id))
}

fn default_delay_model() -> String {
    block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model")
        .to_string()
}

fn default_nam_model() -> String {
    block_nam::supported_models()
        .first()
        .expect("block-nam must expose at least one model")
        .to_string()
}

fn default_preamp_model() -> String {
    block_preamp::supported_models()
        .first()
        .expect("block-preamp must expose at least one model")
        .to_string()
}

fn default_amp_model() -> String {
    block_amp::supported_models()
        .first()
        .expect("block-amp must expose at least one model")
        .to_string()
}

fn default_full_rig_model() -> String {
    block_full_rig::supported_models()
        .first()
        .expect("block-full-rig must expose at least one model")
        .to_string()
}

fn default_cab_model() -> String {
    block_cab::supported_models()
        .first()
        .expect("block-cab must expose at least one model")
        .to_string()
}

fn default_drive_model() -> String {
    block_gain::supported_models()
        .first()
        .expect("block-gain must expose at least one model")
        .to_string()
}

fn default_reverb_model() -> String {
    block_reverb::supported_models()
        .first()
        .expect("block-reverb must expose at least one model")
        .to_string()
}

fn default_utility_model() -> String {
    block_util::supported_models()
        .first()
        .expect("block-util must expose at least one model")
        .to_string()
}

fn default_dynamics_model() -> String {
    block_dyn::supported_models()
        .first()
        .expect("block-dyn must expose at least one model")
        .to_string()
}

fn default_filter_model() -> String {
    block_filter::supported_models()
        .first()
        .expect("block-filter must expose at least one model")
        .to_string()
}

fn default_ir_model() -> String {
    block_ir::supported_models()
        .first()
        .expect("block-ir must expose at least one model")
        .to_string()
}

fn default_wah_model() -> String {
    block_wah::supported_models()
        .first()
        .expect("block-wah must expose at least one model")
        .to_string()
}

fn default_modulation_model() -> String {
    block_mod::supported_models()
        .first()
        .expect("block-mod must expose at least one model")
        .to_string()
}

fn default_pitch_model() -> String {
    block_pitch::supported_models()
        .first()
        .expect("block-pitch must expose at least one model")
        .to_string()
}

const fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{
        load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset, YamlProjectRepository,
    };
    use domain::ids::{BlockId, DeviceId, ChainId};
    use project::block::{
        AudioBlock, AudioBlockKind, CoreBlock, SelectBlock,
    };
    use project::param::ParameterSet;
    use project::project::Project;
    use project::chain::{Chain, ChainOutputMixdown};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn save_project_creates_yaml_that_roundtrips_basic_project() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        let repository = YamlProjectRepository {
            path: project_path.clone(),
        };
        let original = Project {
            name: Some("Test Project".into()),
            device_settings: Vec::new(),
            chains: vec![Chain {
                id: ChainId("chain:0".into()),
                description: Some("Guitar 1".into()),
                enabled: true,
                input_device_id: DeviceId("input-device".into()),
                input_channels: vec![0],
                output_device_id: DeviceId("output-device".into()),
                output_channels: vec![0, 1],
                blocks: Vec::new(),
                output_mixdown: ChainOutputMixdown::Average,
            }],
        };

        repository
            .save_project(&original)
            .expect("project save should succeed");

        assert!(project_path.exists(), "project yaml should be written");

        let loaded = repository
            .load_current_project()
            .expect("saved project should load");

        assert_eq!(loaded.name, original.name);
        assert_eq!(loaded.chains.len(), 1);
        assert_eq!(loaded.chains[0].description, original.chains[0].description);
        assert_eq!(
            loaded.chains[0].input_device_id,
            original.chains[0].input_device_id
        );
        assert_eq!(
            loaded.chains[0].input_channels,
            original.chains[0].input_channels
        );
        assert_eq!(
            loaded.chains[0].output_device_id,
            original.chains[0].output_device_id
        );
        assert_eq!(
            loaded.chains[0].output_channels,
            original.chains[0].output_channels
        );
        assert_eq!(
            loaded.chains[0].output_mixdown,
            original.chains[0].output_mixdown
        );
    }

    #[test]
    fn load_project_ignores_removed_or_invalid_blocks() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        let valid_delay_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        fs::write(
            &project_path,
            format!(
                r#"
chains:
  - enabled: true
    input_device_id: input-device
    input_channels: [0]
    output_device_id: output-device
    output_channels: [0]
    blocks:
      - type: core_nam
        enabled: true
        model_id: legacy
      - type: delay
        enabled: true
        model: {valid_delay_model}
        params:
          time_ms: 200
          feedback: 0.5
          mix: 0.3
"#,
            ),
        )
        .expect("project yaml should be written");

        let repository = YamlProjectRepository { path: project_path };
        let project = repository
            .load_current_project()
            .expect("project should load while skipping invalid blocks");

        assert_eq!(project.chains.len(), 1);
        assert_eq!(project.chains[0].blocks.len(), 1);
        assert_eq!(
            project.chains[0].blocks[0]
                .model_ref()
                .expect("remaining block should expose model")
                .model,
            *valid_delay_model
        );
    }

    #[test]
    fn load_preset_ignores_unknown_models() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let preset_path: PathBuf = temp_dir.path().join("example.yaml");
        let valid_delay_model = block_delay::supported_models()
            .first()
            .expect("block-delay must expose at least one model");
        fs::write(
            &preset_path,
            format!(
                r#"
id: example
blocks:
  - type: delay
    model: deleted_model
    params:
      time_ms: 200
      feedback: 0.5
      mix: 0.3
  - type: delay
    model: {valid_delay_model}
    params:
      time_ms: 210
      feedback: 0.4
      mix: 0.25
"#,
            ),
        )
        .expect("preset yaml should be written");

        let preset = load_chain_preset_file(&preset_path)
            .expect("preset should load while skipping invalid blocks");

        assert_eq!(preset.blocks.len(), 1);
        assert_eq!(
            preset.blocks[0]
                .model_ref()
                .expect("remaining block should expose model")
                .model,
            *valid_delay_model
        );
    }

    #[test]
    fn load_project_supports_generic_select_options() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let project_path = temp_dir.path().join("project.yaml");
        let delay_models = block_delay::supported_models();
        let first_model = delay_models
            .first()
            .expect("block-delay must expose at least one model");
        let second_model = delay_models
            .get(1)
            .unwrap_or(first_model);

        fs::write(
            &project_path,
            format!(
                r#"
chains:
  - enabled: true
    input_device_id: input-device
    input_channels: [0]
    output_device_id: output-device
    output_channels: [0]
    blocks:
      - type: select
        enabled: true
        selected: delay_b
        options:
          - id: delay_a
            type: delay
            model: {first_model}
            params:
              time_ms: 120
              feedback: 0.2
              mix: 0.3
          - id: delay_b
            type: delay
            model: {second_model}
            params:
              time_ms: 240
              feedback: 0.4
              mix: 0.25
"#,
            ),
        )
        .expect("project yaml should be written");

        let repository = YamlProjectRepository { path: project_path };
        let project = repository
            .load_current_project()
            .expect("project should load generic select blocks");

        let select = match &project.chains[0].blocks[0].kind {
            AudioBlockKind::Select(select) => select,
            other => panic!("expected select block, got {:?}", other),
        };
        assert_eq!(select.options.len(), 2);
        assert_eq!(select.selected_block_id.0, "chain:0:block:0::delay_b");
    }

    #[test]
    fn preset_roundtrips_generic_select_options() {
        let temp_dir = tempdir().expect("temp dir should be created");
        let preset_path: PathBuf = temp_dir.path().join("select.yaml");
        let delay_models = block_delay::supported_models();
        let first_model = delay_models
            .first()
            .expect("block-delay must expose at least one model");
        let second_model = delay_models
            .get(1)
            .unwrap_or(first_model);
        let preset = ChainBlocksPreset {
            id: "select".into(),
            name: Some("Delay Select".into()),
            blocks: vec![AudioBlock {
                id: BlockId("preset:select:block:0".into()),
                enabled: true,
                kind: AudioBlockKind::Select(SelectBlock {
                    selected_block_id: BlockId("preset:select:block:0::delay_b".into()),
                    options: vec![
                        delay_block("preset:select:block:0::delay_a", first_model, 120.0),
                        delay_block("preset:select:block:0::delay_b", second_model, 240.0),
                    ],
                }),
            }],
        };

        save_chain_preset_file(&preset_path, &preset).expect("preset save should succeed");
        let raw = fs::read_to_string(&preset_path).expect("saved preset should be readable");
        assert!(raw.contains("type: select"));
        assert!(raw.contains("- id: delay_a"));
        assert!(raw.contains("- id: delay_b"));

        let loaded = load_chain_preset_file(&preset_path).expect("preset should reload");
        let select = match &loaded.blocks[0].kind {
            AudioBlockKind::Select(select) => select,
            other => panic!("expected select block, got {:?}", other),
        };
        assert_eq!(select.selected_block_id.0, "preset:select:block:0::delay_b");
        assert_eq!(select.options.len(), 2);
    }

    fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
        let schema =
            project::block::schema_for_block_model("delay", model).expect("delay schema exists");
        let mut params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("delay defaults should normalize");
        params.insert("time_ms", domain::value_objects::ParameterValue::Float(time_ms));
        AudioBlock {
            id: BlockId(id.into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "delay".to_string(),
                model: model.to_string(),
                params,
            }),
        }
    }
}
