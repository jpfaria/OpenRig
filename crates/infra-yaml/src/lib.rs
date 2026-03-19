use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, TrackId};
use domain::value_objects::ParameterValue;
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use project::block::{
    normalize_block_params, AmpComboBlock, AmpHeadBlock, AudioBlock, AudioBlockKind,
    CompressorBlock, CoreBlock, CoreBlockKind, CoreNamBlock, DelayBlock, DriveBlock, EqBlock,
    FullRigBlock, GateBlock, NamBlock, ReverbBlock, SelectBlock, TremoloBlock, TunerBlock,
};
use project::device::DeviceSettings;
use project::param::ParameterSet;
use project::project::Project;
use project::track::{Track, TrackOutputMixdown};
use stage_amp_head::marshall_jcm_800::MODEL_ID as DEFAULT_AMP_HEAD_MODEL;
use stage_amp_combo::bogner_ecstasy::MODEL_ID as DEFAULT_AMP_COMBO_MODEL;
use stage_delay::digital_basic::MODEL_ID as DEFAULT_DELAY_MODEL;
use stage_dyn::compressor_studio_clean::MODEL_ID as DEFAULT_COMPRESSOR_MODEL;
use stage_dyn::gate_basic::MODEL_ID as DEFAULT_GATE_MODEL;
use stage_filter::eq_three_band_basic::MODEL_ID as DEFAULT_EQ_MODEL;
use stage_full_rig::roland_jc_120b_jazz_chorus::MODEL_ID as DEFAULT_FULL_RIG_MODEL;
use stage_gain::blues_overdrive_bd_2::MODEL_ID as DEFAULT_DRIVE_MODEL;
use stage_mod::tremolo_sine::MODEL_ID as DEFAULT_TREMOLO_MODEL;
use stage_nam::GENERIC_NAM_MODEL_ID;
use stage_reverb::plate_foundation::MODEL_ID as DEFAULT_REVERB_MODEL;
use stage_util::tuner_chromatic::MODEL_ID as DEFAULT_TUNER_MODEL;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct YamlProjectRepository {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TrackBlocksPreset {
    pub id: String,
    pub name: Option<String>,
    pub blocks: Vec<project::block::AudioBlock>,
}

pub fn load_track_preset_file(path: &PathBuf) -> Result<TrackBlocksPreset> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read preset yaml {:?}", path))?;
    let dto: PresetYaml = serde_yaml::from_str(&raw)
        .with_context(|| format!("failed to parse preset yaml {:?}", path))?;
    dto.into_preset()
}

pub fn save_track_preset_file(path: &PathBuf, preset: &TrackBlocksPreset) -> Result<()> {
    let dto = PresetYaml::from_track_preset(preset)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_yaml::to_string(&dto)?)?;
    Ok(())
}

pub fn serialize_audio_blocks(blocks: &[project::block::AudioBlock]) -> Result<Vec<Value>> {
    blocks
        .iter()
        .map(|block| Ok(serde_yaml::to_value(AudioBlockYaml::from_audio_block(block)?)?))
        .collect()
}

impl YamlProjectRepository {
    pub fn load_current_project(&self) -> Result<Project> {
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        let dto: ProjectYaml = serde_yaml::from_str(&raw)?;
        dto.into_project()
    }

    pub fn save_project(&self, _project: &Project) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ProjectYaml {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    device_settings: Vec<DeviceSettingsYaml>,
    tracks: Vec<TrackYaml>,
}

impl ProjectYaml {
    fn into_project(self) -> Result<Project> {
        Ok(Project {
            name: self.name,
            device_settings: self.device_settings.into_iter().map(Into::into).collect(),
            tracks: self
                .tracks
                .into_iter()
                .enumerate()
                .map(|(index, track)| track.into_track(index))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct PresetYaml {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "stages")]
    blocks: Vec<AudioBlockYaml>,
}

impl PresetYaml {
    fn into_preset(self) -> Result<TrackBlocksPreset> {
        Ok(TrackBlocksPreset {
            id: self.id.clone(),
            name: self.name,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .map(|(index, block)| block.into_audio_block(&generated_preset_track_id(&self.id), index))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn from_track_preset(preset: &TrackBlocksPreset) -> Result<Self> {
        Ok(Self {
            id: preset.id.clone(),
            name: preset.name.clone(),
            blocks: preset
                .blocks
                .iter()
                .map(AudioBlockYaml::from_audio_block)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
struct TrackYaml {
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "default_enabled")]
    enabled: bool,
    input_device_id: String,
    input_channels: Vec<usize>,
    output_device_id: String,
    output_channels: Vec<usize>,
    #[serde(default, alias = "stages")]
    blocks: Vec<AudioBlockYaml>,
    #[serde(default)]
    output_mixdown: TrackOutputMixdown,
}

impl TrackYaml {
    fn into_track(self, index: usize) -> Result<Track> {
        let track_id = generated_track_id(index);
        Ok(Track {
            id: track_id.clone(),
            description: self.description,
            enabled: self.enabled,
            input_device_id: DeviceId(self.input_device_id),
            input_channels: self.input_channels,
            output_device_id: DeviceId(self.output_device_id),
            output_channels: self.output_channels,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .map(|(block_index, block)| block.into_audio_block(&track_id, block_index))
                .collect::<Result<Vec<_>>>()?,
            output_mixdown: self.output_mixdown,
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AudioBlockYaml {
    #[serde(rename = "amp-head", alias = "amp_head", alias = "amp")]
    AmpHead {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_amp_head_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "amp-combo", alias = "amp_combo")]
    AmpCombo {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_amp_combo_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "full-rig", alias = "full_rig")]
    FullRig {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_full_rig_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Drive {
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
    Tuner {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_tuner_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Compressor {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_compressor_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Gate {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_gate_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Eq {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_eq_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Tremolo {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_tremolo_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    CoreNam {
        #[serde(default = "default_enabled")]
        enabled: bool,
        model_id: String,
        #[serde(default)]
        ir_id: Option<String>,
    },
    Select {
        #[serde(default = "default_enabled")]
        enabled: bool,
        id: String,
        selected: String,
        options: HashMap<String, SelectOptionYaml>,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SelectOptionYaml {
    Nam {
        #[serde(default = "default_enabled")]
        enabled: bool,
        #[serde(default = "default_nam_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
}

impl AudioBlockYaml {
    fn into_audio_block(self, track_id: &TrackId, index: usize) -> Result<AudioBlock> {
        let generated_id = generated_block_id(track_id, index);

        match self {
            AudioBlockYaml::AmpHead {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::AmpHead(AmpHeadBlock {
                        model: model.clone(),
                        params: load_model_params("amp_head", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::AmpCombo {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::AmpCombo(AmpComboBlock {
                        model: model.clone(),
                        params: load_model_params("amp_combo", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::FullRig {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::FullRig(FullRigBlock {
                        model: model.clone(),
                        params: load_model_params("full_rig", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Drive {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Drive(DriveBlock {
                        model: model.clone(),
                        params: load_model_params("drive", &model, params)?,
                    }),
                }),
            }),
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
            AudioBlockYaml::Delay {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Delay(DelayBlock {
                        model: model.clone(),
                        params: load_model_params("delay", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Reverb {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Reverb(ReverbBlock {
                        model: model.clone(),
                        params: load_model_params("reverb", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Tuner {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tuner(TunerBlock {
                        model: model.clone(),
                        params: load_model_params("tuner", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Compressor {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Compressor(CompressorBlock {
                        model: model.clone(),
                        params: load_model_params("compressor", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Gate {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Gate(GateBlock {
                        model: model.clone(),
                        params: load_model_params("gate", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Eq {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Eq(EqBlock {
                        model: model.clone(),
                        params: load_model_params("eq", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Tremolo {
                enabled,
                model,
                params,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tremolo(TremoloBlock {
                        model: model.clone(),
                        params: load_model_params("tremolo", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::CoreNam {
                enabled,
                model_id,
                ir_id,
            } => Ok(AudioBlock {
                id: generated_id,
                enabled,
                kind: AudioBlockKind::CoreNam(CoreNamBlock { model_id, ir_id }),
            }),
            AudioBlockYaml::Select {
                enabled,
                id,
                selected,
                options,
            } => {
                let selected_block_id = BlockId(format!("{}::{}", id, selected));
                let options = options
                    .into_iter()
                    .map(|(name, option)| {
                        let option_id = BlockId(format!("{}::{}", id, name));
                        match option {
                            SelectOptionYaml::Nam {
                                enabled,
                                model,
                                params,
                            } => Ok(AudioBlock {
                                id: option_id,
                                enabled,
                                kind: AudioBlockKind::Nam(NamBlock {
                                    model: model.clone(),
                                    params: load_model_params("nam", &model, params)?,
                                }),
                            }),
                        }
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(AudioBlock {
                    id: BlockId(id),
                    enabled,
                    kind: AudioBlockKind::Select(SelectBlock {
                        selected_block_id,
                        options,
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
            AudioBlockKind::Core(core) => match &core.kind {
                CoreBlockKind::AmpHead(stage) => Ok(Self::AmpHead {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::AmpCombo(stage) => Ok(Self::AmpCombo {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::FullRig(stage) => Ok(Self::FullRig {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Drive(stage) => Ok(Self::Drive {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Delay(stage) => Ok(Self::Delay {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Reverb(stage) => Ok(Self::Reverb {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Tuner(stage) => Ok(Self::Tuner {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Compressor(stage) => Ok(Self::Compressor {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Gate(stage) => Ok(Self::Gate {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Eq(stage) => Ok(Self::Eq {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                CoreBlockKind::Tremolo(stage) => Ok(Self::Tremolo {
                    enabled: block.enabled,
                    model: stage.model.clone(),
                    params: parameter_set_to_yaml_value(&stage.params),
                }),
                unsupported => Err(anyhow!(
                    "unsupported block kind for yaml export: {:?}",
                    unsupported
                )),
            },
            AudioBlockKind::CoreNam(stage) => Ok(Self::CoreNam {
                enabled: block.enabled,
                model_id: stage.model_id.clone(),
                ir_id: stage.ir_id.clone(),
            }),
            AudioBlockKind::Select(select) => {
                let id = block.id.0.clone();
                let selected = select
                    .selected_block_id
                    .0
                    .rsplit("::")
                    .next()
                    .unwrap_or(select.selected_block_id.0.as_str())
                    .to_string();
                let mut options = HashMap::new();
                for option in &select.options {
                    let name = option
                        .id
                        .0
                        .rsplit("::")
                        .next()
                        .unwrap_or(option.id.0.as_str())
                        .to_string();
                    match &option.kind {
                        AudioBlockKind::Nam(stage) => {
                            options.insert(
                                name,
                                SelectOptionYaml::Nam {
                                    enabled: option.enabled,
                                    model: stage.model.clone(),
                                    params: parameter_set_to_yaml_value(&stage.params),
                                },
                            );
                        }
                        unsupported => {
                            return Err(anyhow!(
                                "unsupported select option kind for yaml export: {:?}",
                                unsupported
                            ));
                        }
                    }
                }

                Ok(Self::Select {
                    enabled: block.enabled,
                    id,
                    selected,
                    options,
                })
            }
        }
    }
}

fn load_model_params(effect_type: &str, model: &str, raw_params: Value) -> Result<ParameterSet> {
    let flattened = flatten_parameter_set(raw_params)?;
    normalize_block_params(effect_type, model, flattened).map_err(anyhow::Error::msg)
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

fn generated_block_id(track_id: &TrackId, index: usize) -> BlockId {
    BlockId(format!("{}:block:{}", track_id.0, index))
}

fn generated_track_id(index: usize) -> TrackId {
    TrackId(format!("track:{}", index))
}

fn generated_preset_track_id(preset_id: &str) -> TrackId {
    TrackId(format!("preset:{}", preset_id))
}

fn default_delay_model() -> String {
    DEFAULT_DELAY_MODEL.to_string()
}

fn default_nam_model() -> String {
    GENERIC_NAM_MODEL_ID.to_string()
}

fn default_amp_head_model() -> String {
    DEFAULT_AMP_HEAD_MODEL.to_string()
}

fn default_amp_combo_model() -> String {
    DEFAULT_AMP_COMBO_MODEL.to_string()
}

fn default_full_rig_model() -> String {
    DEFAULT_FULL_RIG_MODEL.to_string()
}

fn default_drive_model() -> String {
    DEFAULT_DRIVE_MODEL.to_string()
}

fn default_reverb_model() -> String {
    DEFAULT_REVERB_MODEL.to_string()
}

fn default_tuner_model() -> String {
    DEFAULT_TUNER_MODEL.to_string()
}

fn default_compressor_model() -> String {
    DEFAULT_COMPRESSOR_MODEL.to_string()
}

fn default_gate_model() -> String {
    DEFAULT_GATE_MODEL.to_string()
}

fn default_eq_model() -> String {
    DEFAULT_EQ_MODEL.to_string()
}

fn default_tremolo_model() -> String {
    DEFAULT_TREMOLO_MODEL.to_string()
}

const fn default_enabled() -> bool {
    true
}
