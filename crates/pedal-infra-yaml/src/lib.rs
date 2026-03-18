use anyhow::{Context, Result};
use pedal-domain::ids::{BlockId, DeviceId, InputId, OutputId, SetupId, TrackId};
use pedal-ports::{PresetRepository, SetupRepository, StateRepository};
use pedal-preset::Preset;
use pedal-setup::block::*;
use pedal-setup::device::{InputDevice, OutputDevice};
use pedal-setup::io::{Input, Output};
use pedal-setup::setup::Setup;
use pedal-setup::track::Track;
use pedal-state::pedalboard_state::PedalboardState;
use serde::Deserialize;
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
            .with_context(|| format!("falha ao ler yaml {:?}", self.path))?;
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
            .with_context(|| format!("falha ao ler yaml {:?}", self.path))?;
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
            .with_context(|| format!("falha ao ler yaml {:?}", self.path))?;
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
    id: String,
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

#[derive(Debug, Deserialize)]
struct InputDeviceYaml { id: String, match_name: String, sample_rate: u32, buffer_size_frames: u32 }
impl From<InputDeviceYaml> for InputDevice {
    fn from(v: InputDeviceYaml) -> Self {
        Self { id: DeviceId(v.id), match_name: v.match_name, sample_rate: v.sample_rate, buffer_size_frames: v.buffer_size_frames }
    }
}
#[derive(Debug, Deserialize)]
struct OutputDeviceYaml { id: String, match_name: String, sample_rate: u32, buffer_size_frames: u32 }
impl From<OutputDeviceYaml> for OutputDevice {
    fn from(v: OutputDeviceYaml) -> Self {
        Self { id: DeviceId(v.id), match_name: v.match_name, sample_rate: v.sample_rate, buffer_size_frames: v.buffer_size_frames }
    }
}
#[derive(Debug, Deserialize)]
struct InputYaml { id: String, device_id: String, channels: Vec<usize> }
impl From<InputYaml> for Input {
    fn from(v: InputYaml) -> Self { Self { id: InputId(v.id), device_id: DeviceId(v.device_id), channels: v.channels } }
}
#[derive(Debug, Deserialize)]
struct OutputYaml { id: String, device_id: String, channels: Vec<usize> }
impl From<OutputYaml> for Output {
    fn from(v: OutputYaml) -> Self { Self { id: OutputId(v.id), device_id: DeviceId(v.device_id), channels: v.channels } }
}
#[derive(Debug, Deserialize)]
struct TrackYaml {
    id: String,
    input_id: String,
    outputs: Vec<String>,
    gain: f32,
    #[serde(default)]
    blocks: Vec<AudioBlockYaml>,
}
impl From<TrackYaml> for Track {
    fn from(v: TrackYaml) -> Self {
        Self {
            id: TrackId(v.id),
            input_id: InputId(v.input_id),
            output_ids: v.outputs.into_iter().map(OutputId).collect(),
            gain: v.gain,
            blocks: v.blocks.into_iter().map(Into::into).collect(),
        }
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AudioBlockYaml {
    Nam { id: String, model_path: String, ir_path: Option<String> },
    CoreNam { id: String, model_id: String, ir_id: Option<String> },
}
impl From<AudioBlockYaml> for AudioBlock {
    fn from(v: AudioBlockYaml) -> Self {
        match v {
            AudioBlockYaml::Nam { id, model_path, ir_path } => AudioBlock {
                id: BlockId(id),
                kind: AudioBlockKind::Nam(NamBlock { model_path, ir_path }),
            },
            AudioBlockYaml::CoreNam { id, model_id, ir_id } => AudioBlock {
                id: BlockId(id),
                kind: AudioBlockKind::CoreNam(CoreNamBlock { model_id, ir_id }),
            },
        }
    }
}
