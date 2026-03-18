use anyhow::{Context, Result};
use domain::ids::{BlockId, DeviceId, InputId, OutputId, SetupId, TrackId};
use ports::{PresetRepository, SetupRepository, StateRepository};
use preset::Preset;
use serde::Deserialize;
use setup::block::{
    AudioBlock, AudioBlockKind, CompressorBlock, CompressorParams, CoreBlock, CoreBlockKind,
    CoreNamBlock, DelayBlock, DelayParams, EqBlock, EqParams, GateBlock, GateParams, NamBlock,
    NamEqParams, NamNoiseGateParams, NamParams, ReverbBlock, ReverbParams, SelectBlock,
    TremoloBlock, TremoloParams, TunerBlock, TunerParams,
};
use setup::device::{InputDevice, OutputDevice};
use setup::io::{Input, Output};
use setup::setup::Setup;
use setup::track::{Track, TrackBusMode, TrackOutputMixdown};
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
        Ok(dto.into_setup())
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
    fn into_setup(self) -> Setup {
        Setup {
            id: SetupId(self.id),
            name: self.name,
            input_devices: self.input_devices.into_iter().map(Into::into).collect(),
            output_devices: self.output_devices.into_iter().map(Into::into).collect(),
            inputs: self.inputs.into_iter().map(Into::into).collect(),
            outputs: self.outputs.into_iter().map(Into::into).collect(),
            tracks: self.tracks.into_iter().map(Into::into).collect(),
        }
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
    bus_mode: TrackBusMode,
    #[serde(default)]
    output_mixdown: TrackOutputMixdown,
    gain: f32,
    #[serde(default, alias = "stages")]
    blocks: Vec<AudioBlockYaml>,
}
impl From<TrackYaml> for Track {
    fn from(value: TrackYaml) -> Self {
        let track_id = TrackId(value.id.clone());
        Self {
            id: track_id.clone(),
            input_id: InputId(value.input_id),
            output_ids: value.outputs.into_iter().map(OutputId).collect(),
            bus_mode: value.bus_mode,
            output_mixdown: value.output_mixdown,
            gain: value.gain,
            blocks: value
                .blocks
                .into_iter()
                .enumerate()
                .map(|(index, block)| block.into_audio_block(&track_id, index))
                .collect(),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AudioBlockYaml {
    Nam {
        #[serde(default = "default_nam_model")]
        model: String,
        params: NamParamsYaml,
    },
    Delay {
        #[serde(default = "default_delay_model")]
        model: String,
        params: DelayParamsYaml,
    },
    Reverb {
        #[serde(default = "default_reverb_model")]
        model: String,
        params: ReverbParamsYaml,
    },
    Tuner {
        #[serde(default = "default_tuner_model")]
        model: String,
        params: TunerParamsYaml,
    },
    Compressor {
        #[serde(default = "default_compressor_model")]
        model: String,
        params: CompressorParamsYaml,
    },
    Gate {
        #[serde(default = "default_gate_model")]
        model: String,
        params: GateParamsYaml,
    },
    Eq {
        #[serde(default = "default_eq_model")]
        model: String,
        params: EqParamsYaml,
    },
    Tremolo {
        #[serde(default = "default_tremolo_model")]
        model: String,
        params: TremoloParamsYaml,
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
        params: NamParamsYaml,
    },
}

#[derive(Debug, Default, Deserialize)]
struct NamParamsYaml {
    model_path: String,
    ir_path: Option<String>,
    #[serde(default)]
    input_db: Option<f32>,
    #[serde(default)]
    output_db: Option<f32>,
    #[serde(default)]
    noise_gate: Option<NamNoiseGateYaml>,
    #[serde(default)]
    eq: Option<NamEqYaml>,
    #[serde(default)]
    ir_enabled: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct NamNoiseGateYaml {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, alias = "threshold")]
    threshold_db: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct NamEqYaml {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    bass: Option<f32>,
    #[serde(default)]
    middle: Option<f32>,
    #[serde(default)]
    treble: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct DelayParamsYaml {
    time_ms: Option<f32>,
    feedback: Option<f32>,
    mix: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct ReverbParamsYaml {
    room_size: Option<f32>,
    damping: Option<f32>,
    mix: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct TunerParamsYaml {
    #[serde(default = "default_reference_hz")]
    reference_hz: f32,
}

#[derive(Debug, Default, Deserialize)]
struct CompressorParamsYaml {
    threshold: Option<f32>,
    ratio: Option<f32>,
    attack_ms: Option<f32>,
    release_ms: Option<f32>,
    makeup_gain_db: Option<f32>,
    mix: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct GateParamsYaml {
    threshold: Option<f32>,
    attack_ms: Option<f32>,
    release_ms: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct EqParamsYaml {
    low_gain_db: Option<f32>,
    mid_gain_db: Option<f32>,
    high_gain_db: Option<f32>,
}

#[derive(Debug, Default, Deserialize)]
struct TremoloParamsYaml {
    rate_hz: Option<f32>,
    depth: Option<f32>,
}
impl AudioBlockYaml {
    fn into_audio_block(self, track_id: &TrackId, index: usize) -> AudioBlock {
        let generated_id = generated_block_id(track_id, index);

        match self {
            AudioBlockYaml::Nam { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Nam(NamBlock {
                    model,
                    params: NamParams {
                        model_path: params.model_path,
                        ir_path: params.ir_path,
                        input_db: params.input_db.unwrap_or(0.0),
                        output_db: params.output_db.unwrap_or(0.0),
                        noise_gate: NamNoiseGateParams {
                            enabled: params
                                .noise_gate
                                .as_ref()
                                .and_then(|value| value.enabled)
                                .unwrap_or(true),
                            threshold_db: params
                                .noise_gate
                                .as_ref()
                                .and_then(|value| value.threshold_db)
                                .unwrap_or(-80.0),
                        },
                        eq: NamEqParams {
                            enabled: params
                                .eq
                                .as_ref()
                                .and_then(|value| value.enabled)
                                .unwrap_or(true),
                            bass: params
                                .eq
                                .as_ref()
                                .and_then(|value| value.bass)
                                .unwrap_or(5.0),
                            middle: params
                                .eq
                                .as_ref()
                                .and_then(|value| value.middle)
                                .unwrap_or(5.0),
                            treble: params
                                .eq
                                .as_ref()
                                .and_then(|value| value.treble)
                                .unwrap_or(5.0),
                        },
                        ir_enabled: params.ir_enabled.unwrap_or(true),
                    },
                }),
            },
            AudioBlockYaml::Delay { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Delay(DelayBlock {
                        model,
                        params: DelayParams {
                            time_ms: params.time_ms.unwrap_or(380.0),
                            feedback: params.feedback.unwrap_or(0.35),
                            mix: params.mix.unwrap_or(0.3),
                        },
                    }),
                }),
            },
            AudioBlockYaml::Reverb { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Reverb(ReverbBlock {
                        model,
                        params: ReverbParams {
                            room_size: params.room_size.unwrap_or(0.45),
                            damping: params.damping.unwrap_or(0.35),
                            mix: params.mix.unwrap_or(0.25),
                        },
                    }),
                }),
            },
            AudioBlockYaml::Tuner { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tuner(TunerBlock {
                        model,
                        params: TunerParams {
                            reference_hz: params.reference_hz,
                        },
                    }),
                }),
            },
            AudioBlockYaml::Compressor { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Compressor(CompressorBlock {
                        model,
                        params: CompressorParams {
                            threshold: params.threshold.unwrap_or(-18.0),
                            ratio: params.ratio.unwrap_or(4.0),
                            attack_ms: params.attack_ms.unwrap_or(10.0),
                            release_ms: params.release_ms.unwrap_or(80.0),
                            makeup_gain_db: params.makeup_gain_db.unwrap_or(0.0),
                            mix: params.mix.unwrap_or(1.0),
                        },
                    }),
                }),
            },
            AudioBlockYaml::Gate { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Gate(GateBlock {
                        model,
                        params: GateParams {
                            threshold: params.threshold.unwrap_or(-60.0),
                            attack_ms: params.attack_ms.unwrap_or(5.0),
                            release_ms: params.release_ms.unwrap_or(50.0),
                        },
                    }),
                }),
            },
            AudioBlockYaml::Eq { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Eq(EqBlock {
                        model,
                        params: EqParams {
                            low_gain_db: params.low_gain_db.unwrap_or(0.0),
                            mid_gain_db: params.mid_gain_db.unwrap_or(0.0),
                            high_gain_db: params.high_gain_db.unwrap_or(0.0),
                        },
                    }),
                }),
            },
            AudioBlockYaml::Tremolo { model, params } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tremolo(TremoloBlock {
                        model,
                        params: TremoloParams {
                            rate_hz: params.rate_hz.unwrap_or(4.0),
                            depth: params.depth.unwrap_or(0.5),
                        },
                    }),
                }),
            },
            AudioBlockYaml::CoreNam { model_id, ir_id } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::CoreNam(CoreNamBlock { model_id, ir_id }),
            },
            AudioBlockYaml::Select {
                id,
                selected,
                options,
            } => {
                let selected_block_id = BlockId(format!("{}::{}", id, selected));
                let options = options
                    .into_iter()
                    .map(|(name, option)| AudioBlock {
                        id: BlockId(format!("{}::{}", id, name)),
                        kind: match option {
                            SelectOptionYaml::Nam { model, params } => {
                                AudioBlockKind::Nam(NamBlock {
                                    model,
                                    params: NamParams {
                                        model_path: params.model_path,
                                        ir_path: params.ir_path,
                                        input_db: params.input_db.unwrap_or(0.0),
                                        output_db: params.output_db.unwrap_or(0.0),
                                        noise_gate: NamNoiseGateParams {
                                            enabled: params
                                                .noise_gate
                                                .as_ref()
                                                .and_then(|value| value.enabled)
                                                .unwrap_or(true),
                                            threshold_db: params
                                                .noise_gate
                                                .as_ref()
                                                .and_then(|value| value.threshold_db)
                                                .unwrap_or(-80.0),
                                        },
                                        eq: NamEqParams {
                                            enabled: params
                                                .eq
                                                .as_ref()
                                                .and_then(|value| value.enabled)
                                                .unwrap_or(true),
                                            bass: params
                                                .eq
                                                .as_ref()
                                                .and_then(|value| value.bass)
                                                .unwrap_or(5.0),
                                            middle: params
                                                .eq
                                                .as_ref()
                                                .and_then(|value| value.middle)
                                                .unwrap_or(5.0),
                                            treble: params
                                                .eq
                                                .as_ref()
                                                .and_then(|value| value.treble)
                                                .unwrap_or(5.0),
                                        },
                                        ir_enabled: params.ir_enabled.unwrap_or(true),
                                    },
                                })
                            }
                        },
                    })
                    .collect();
                AudioBlock {
                    id: BlockId(id),
                    kind: AudioBlockKind::Select(SelectBlock {
                        selected_block_id,
                        options,
                    }),
                }
            }
        }
    }
}

fn generated_block_id(track_id: &TrackId, index: usize) -> BlockId {
    BlockId(format!("{}:block:{}", track_id.0, index))
}

fn default_delay_model() -> String {
    "digital_basic".to_string()
}

fn default_nam_model() -> String {
    "neural_amp_modeler".to_string()
}

fn default_reverb_model() -> String {
    "plate_foundation".to_string()
}

fn default_tuner_model() -> String {
    "chromatic_basic".to_string()
}

fn default_compressor_model() -> String {
    "studio_clean".to_string()
}

fn default_gate_model() -> String {
    "noise_gate_basic".to_string()
}

fn default_eq_model() -> String {
    "three_band_basic".to_string()
}

fn default_tremolo_model() -> String {
    "sine_tremolo".to_string()
}

fn default_reference_hz() -> f32 {
    440.0
}
