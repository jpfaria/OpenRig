use anyhow::{anyhow, Context, Result};
use domain::ids::{BlockId, DeviceId, InputId, OutputId, SetupId, TrackId};
use domain::value_objects::ParameterValue;
use ports::{PresetRepository, SetupRepository, StateRepository};
use preset::Preset;
use serde::Deserialize;
use serde_yaml::Value;
use setup::block::{
    normalize_block_params, AmpBlock, AudioBlock, AudioBlockKind, CompressorBlock, CoreBlock,
    CoreBlockKind, CoreNamBlock, DelayBlock, EqBlock, FullRigBlock, GateBlock, NamBlock, ReverbBlock,
    SelectBlock, TremoloBlock, TunerBlock,
};
use setup::device::{InputDevice, OutputDevice};
use setup::io::{Input, Output};
use setup::param::ParameterSet;
use setup::setup::Setup;
use setup::track::{Track, TrackOutputMixdown};
use stage_amp::DEFAULT_AMP_MODEL;
use stage_delay::DEFAULT_DELAY_MODEL;
use stage_dyn::DEFAULT_COMPRESSOR_MODEL;
use stage_dyn::DEFAULT_GATE_MODEL;
use stage_filter::DEFAULT_EQ_MODEL;
use stage_full_rig::DEFAULT_FULL_RIG_MODEL;
use stage_mod::DEFAULT_TREMOLO_MODEL;
use stage_nam::DEFAULT_NAM_MODEL;
use stage_reverb::DEFAULT_REVERB_MODEL;
use stage_util::DEFAULT_TUNER_MODEL;
use state::pedalboard_state::PedalboardState;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct YamlSetupRepository {
    pub path: PathBuf,
}

pub struct YamlStateRepository {
    pub path: PathBuf,
}

pub struct YamlPresetRepository {
    pub path: PathBuf,
}

impl SetupRepository for YamlSetupRepository {
    fn load_current_setup(&self) -> Result<Setup> {
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        let dto: SetupYaml = serde_yaml::from_str(&raw)?;
        dto.into_setup()
    }

    fn save_setup(&self, _setup: &Setup) -> Result<()> {
        Ok(())
    }
}

impl StateRepository for YamlStateRepository {
    fn load_state(&self) -> Result<PedalboardState> {
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        Ok(serde_yaml::from_str(&raw)?)
    }

    fn save_state(&self, state: &PedalboardState) -> Result<()> {
        let raw = serde_yaml::to_string(state)?;
        fs::write(&self.path, raw)?;
        Ok(())
    }
}

impl PresetRepository for YamlPresetRepository {
    fn load_preset(&self, _preset_id: &str) -> Result<Preset> {
        let raw = fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read yaml {:?}", self.path))?;
        Ok(serde_yaml::from_str(&raw)?)
    }

    fn save_preset(&self, preset: &Preset) -> Result<()> {
        let raw = serde_yaml::to_string(preset)?;
        fs::write(&self.path, raw)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct SetupYaml {
    #[serde(default = "default_setup_id")]
    id: String,
    #[serde(default = "default_setup_name")]
    name: String,
    input_devices: Vec<InputDeviceYaml>,
    output_devices: Vec<OutputDeviceYaml>,
    inputs: Vec<InputYaml>,
    outputs: Vec<OutputYaml>,
    tracks: Vec<TrackYaml>,
}

impl SetupYaml {
    fn into_setup(self) -> Result<Setup> {
        Ok(Setup {
            id: SetupId(self.id),
            name: self.name,
            input_devices: self.input_devices.into_iter().map(Into::into).collect(),
            output_devices: self.output_devices.into_iter().map(Into::into).collect(),
            inputs: self.inputs.into_iter().map(Into::into).collect(),
            outputs: self.outputs.into_iter().map(Into::into).collect(),
            tracks: self
                .tracks
                .into_iter()
                .map(TrackYaml::into_track)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

fn default_setup_id() -> String {
    "default-setup".to_string()
}

fn default_setup_name() -> String {
    "Default Setup".to_string()
}

#[derive(Debug, Deserialize)]
struct InputDeviceYaml {
    id: String,
    match_name: String,
    sample_rate: u32,
    buffer_size_frames: u32,
}

impl From<InputDeviceYaml> for InputDevice {
    fn from(value: InputDeviceYaml) -> Self {
        Self {
            id: DeviceId(value.id),
            match_name: value.match_name,
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OutputDeviceYaml {
    id: String,
    match_name: String,
    sample_rate: u32,
    buffer_size_frames: u32,
}

impl From<OutputDeviceYaml> for OutputDevice {
    fn from(value: OutputDeviceYaml) -> Self {
        Self {
            id: DeviceId(value.id),
            match_name: value.match_name,
            sample_rate: value.sample_rate,
            buffer_size_frames: value.buffer_size_frames,
        }
    }
}

#[derive(Debug, Deserialize)]
struct InputYaml {
    id: String,
    device_id: String,
    channels: Vec<usize>,
}

impl From<InputYaml> for Input {
    fn from(value: InputYaml) -> Self {
        Self {
            id: InputId(value.id),
            device_id: DeviceId(value.device_id),
            channels: value.channels,
        }
    }
}

#[derive(Debug, Deserialize)]
struct OutputYaml {
    id: String,
    device_id: String,
    channels: Vec<usize>,
}

impl From<OutputYaml> for Output {
    fn from(value: OutputYaml) -> Self {
        Self {
            id: OutputId(value.id),
            device_id: DeviceId(value.device_id),
            channels: value.channels,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TrackYaml {
    id: String,
    input_id: String,
    outputs: Vec<String>,
    #[serde(default)]
    output_mixdown: TrackOutputMixdown,
    gain: f32,
    #[serde(default, alias = "stages")]
    blocks: Vec<AudioBlockYaml>,
}

impl TrackYaml {
    fn into_track(self) -> Result<Track> {
        let track_id = TrackId(self.id.clone());
        Ok(Track {
            id: track_id.clone(),
            input_id: InputId(self.input_id),
            output_ids: self.outputs.into_iter().map(OutputId).collect(),
            output_mixdown: self.output_mixdown,
            gain: self.gain,
            blocks: self
                .blocks
                .into_iter()
                .enumerate()
                .map(|(index, block)| block.into_audio_block(&track_id, index))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AudioBlockYaml {
    Amp {
        #[serde(default = "default_amp_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    #[serde(rename = "full-rig", alias = "full_rig")]
    FullRig {
        #[serde(default = "default_full_rig_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Nam {
        #[serde(default = "default_nam_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Delay {
        #[serde(default = "default_delay_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Reverb {
        #[serde(default = "default_reverb_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Tuner {
        #[serde(default = "default_tuner_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Compressor {
        #[serde(default = "default_compressor_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Gate {
        #[serde(default = "default_gate_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Eq {
        #[serde(default = "default_eq_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    Tremolo {
        #[serde(default = "default_tremolo_model")]
        model: String,
        #[serde(default)]
        params: Value,
    },
    CoreNam {
        model_id: String,
        #[serde(default)]
        ir_id: Option<String>,
    },
    Select {
        id: String,
        selected: String,
        options: HashMap<String, SelectOptionYaml>,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum SelectOptionYaml {
    Nam {
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
            AudioBlockYaml::Amp { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Amp(AmpBlock {
                        model: model.clone(),
                        params: load_model_params("amp", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::FullRig { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::FullRig(FullRigBlock {
                        model: model.clone(),
                        params: load_model_params("full_rig", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Nam { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Nam(NamBlock {
                    model: model.clone(),
                    params: load_model_params("nam", &model, params)?,
                }),
            }),
            AudioBlockYaml::Delay { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Delay(DelayBlock {
                        model: model.clone(),
                        params: load_model_params("delay", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Reverb { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Reverb(ReverbBlock {
                        model: model.clone(),
                        params: load_model_params("reverb", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Tuner { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tuner(TunerBlock {
                        model: model.clone(),
                        params: load_model_params("tuner", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Compressor { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Compressor(CompressorBlock {
                        model: model.clone(),
                        params: load_model_params("compressor", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Gate { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Gate(GateBlock {
                        model: model.clone(),
                        params: load_model_params("gate", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Eq { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Eq(EqBlock {
                        model: model.clone(),
                        params: load_model_params("eq", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::Tremolo { model, params } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tremolo(TremoloBlock {
                        model: model.clone(),
                        params: load_model_params("tremolo", &model, params)?,
                    }),
                }),
            }),
            AudioBlockYaml::CoreNam { model_id, ir_id } => Ok(AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::CoreNam(CoreNamBlock { model_id, ir_id }),
            }),
            AudioBlockYaml::Select {
                id,
                selected,
                options,
            } => {
                let selected_block_id = BlockId(format!("{}::{}", id, selected));
                let options = options
                    .into_iter()
                    .map(|(name, option)| {
                        let option_id = BlockId(format!("{}::{}", id, name));
                        let kind = match option {
                            SelectOptionYaml::Nam { model, params } => {
                                AudioBlockKind::Nam(NamBlock {
                                    model: model.clone(),
                                    params: load_model_params("nam", &model, params)?,
                                })
                            }
                        };
                        Ok(AudioBlock {
                            id: option_id,
                            kind,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;

                Ok(AudioBlock {
                    id: BlockId(id),
                    kind: AudioBlockKind::Select(SelectBlock {
                        selected_block_id,
                        options,
                    }),
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

fn default_delay_model() -> String {
    DEFAULT_DELAY_MODEL.to_string()
}

fn default_nam_model() -> String {
    DEFAULT_NAM_MODEL.to_string()
}

fn default_amp_model() -> String {
    DEFAULT_AMP_MODEL.to_string()
}

fn default_full_rig_model() -> String {
    DEFAULT_FULL_RIG_MODEL.to_string()
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
