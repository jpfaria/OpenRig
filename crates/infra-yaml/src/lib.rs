use anyhow::{Context, Result};
use domain::ids::{BlockId, DeviceId, InputId, OutputId, SetupId, TrackId};
use ports::{PresetRepository, SetupRepository, StateRepository};
use preset::Preset;
use setup::block::{
    AudioBlock, AudioBlockKind, CoreBlock, CoreBlockKind, CoreNamBlock, DelayBlock, NamBlock,
    ReverbBlock, SelectBlock, TunerBlock,
};
use setup::device::{InputDevice, OutputDevice};
use setup::io::{Input, Output};
use setup::setup::Setup;
use setup::track::Track;
use state::pedalboard_state::PedalboardState;
use serde::Deserialize;
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
        model_path: String,
        #[serde(default)]
        ir_path: Option<String>,
    },
    Delay {
        #[serde(default = "default_delay_model")]
        model: String,
        time_ms: f32,
        feedback: f32,
        mix: f32,
    },
    Reverb {
        #[serde(default = "default_reverb_model")]
        model: String,
        room_size: f32,
        damping: f32,
        mix: f32,
    },
    Tuner {
        #[serde(default = "default_tuner_model")]
        model: String,
        #[serde(default = "default_reference_hz")]
        reference_hz: f32,
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
        model_path: String,
        #[serde(default)]
        ir_path: Option<String>,
    },
}
impl AudioBlockYaml {
    fn into_audio_block(self, track_id: &TrackId, index: usize) -> AudioBlock {
        let generated_id = generated_block_id(track_id, index);

        match self {
            AudioBlockYaml::Nam {
                model_path,
                ir_path,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Nam(NamBlock { model_path, ir_path }),
            },
            AudioBlockYaml::Delay {
                model,
                time_ms,
                feedback,
                mix,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Delay(DelayBlock {
                        model,
                        time_ms,
                        feedback,
                        mix,
                    }),
                }),
            },
            AudioBlockYaml::Reverb {
                model,
                room_size,
                damping,
                mix,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Reverb(ReverbBlock {
                        model,
                        room_size,
                        damping,
                        mix,
                    }),
                }),
            },
            AudioBlockYaml::Tuner {
                model,
                reference_hz,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tuner(TunerBlock {
                        model,
                        reference_hz,
                    }),
                }),
            },
            AudioBlockYaml::Compressor {
                threshold,
                ratio,
                attack_ms,
                release_ms,
                makeup_gain_db,
                mix,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Compressor(setup::block::CompressorBlock {
                        threshold,
                        ratio,
                        attack_ms,
                        release_ms,
                        makeup_gain_db,
                        mix,
                    }),
                }),
            },
            AudioBlockYaml::Gate {
                threshold,
                attack_ms,
                release_ms,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Gate(setup::block::GateBlock {
                        threshold,
                        attack_ms,
                        release_ms,
                    }),
                }),
            },
            AudioBlockYaml::Eq {
                low_gain_db,
                mid_gain_db,
                high_gain_db,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Eq(setup::block::EqBlock {
                        low_gain_db,
                        mid_gain_db,
                        high_gain_db,
                    }),
                }),
            },
            AudioBlockYaml::Tremolo {
                rate_hz,
                depth,
            } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::Core(CoreBlock {
                    kind: CoreBlockKind::Tremolo(setup::block::TremoloBlock {
                        rate_hz,
                        depth,
                    }),
                }),
            },
            AudioBlockYaml::CoreNam { model_id, ir_id } => AudioBlock {
                id: generated_id,
                kind: AudioBlockKind::CoreNam(CoreNamBlock { model_id, ir_id }),
            },
            AudioBlockYaml::Select { id, selected, options } => {
                let selected_block_id = BlockId(format!("{}::{}", id, selected));
                let options = options
                    .into_iter()
                    .map(|(name, option)| AudioBlock {
                        id: BlockId(format!("{}::{}", id, name)),
                        kind: match option {
                            SelectOptionYaml::Nam { model_path, ir_path } => {
                                AudioBlockKind::Nam(NamBlock { model_path, ir_path })
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
    "native_digital".to_string()
}

fn default_reverb_model() -> String {
    "plate".to_string()
}

fn default_tuner_model() -> String {
    "chromatic".to_string()
}

fn default_reference_hz() -> f32 {
    440.0
}
