use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_pitchotto";
pub const DISPLAY_NAME: &str = "Pitchotto";
const BRAND: &str = "shiro";

const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/pitchotto";
const PLUGIN_DIR: &str = "Pitchotto.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Pitchotto_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Pitchotto_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Pitchotto_dsp.dll";

// LV2 port indices (from Pitchotto_dsp.ttl)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_RATIO2: usize = 2;
const PORT_MIX: usize = 3;
const PORT_DELAY1: usize = 4;
const PORT_RATIO1: usize = 5;
const PORT_CUTOFF: usize = 6;
const PORT_BLUR: usize = 7;
const PORT_DELAY2: usize = 8;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "voice1",
                "Voice 1",
                Some("Pitch"),
                Some(-12.0),
                -12.0,
                12.0,
                0.5,
                ParameterUnit::None,
            ),
            float_parameter(
                "voice2",
                "Voice 2",
                Some("Pitch"),
                Some(12.0),
                -12.0,
                12.0,
                0.5,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Control"),
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "delay1",
                "Delay 1",
                Some("Control"),
                Some(100.0),
                1.0,
                1000.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "delay2",
                "Delay 2",
                Some("Control"),
                Some(100.0),
                1.0,
                1000.0,
                1.0,
                ParameterUnit::None,
            ),
        ],
    }
}

fn semitones_to_ratio(semitones: f32) -> f32 {
    2.0_f32.powf(semitones / 12.0)
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!(
        "LV2 binary '{}' not found in '{}'",
        PLUGIN_BINARY,
        lv2::default_lv2_lib_dir()
    )
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

const MAX_BLOCK: usize = 4096;

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
    left_buf: Box<[f32; MAX_BLOCK]>,
    right_buf: Box<[f32; MAX_BLOCK]>,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }

    fn process_block(&mut self, buffer: &mut [[f32; 2]]) {
        let len = buffer.len().min(MAX_BLOCK);
        for (i, frame) in buffer[..len].iter().enumerate() {
            self.left_buf[i] = frame[0];
            self.right_buf[i] = frame[1];
        }
        self.left.process_block(&mut self.left_buf[..len]);
        self.right.process_block(&mut self.right_buf[..len]);
        for (i, frame) in buffer[..len].iter_mut().enumerate() {
            frame[0] = self.left_buf[i];
            frame[1] = self.right_buf[i];
        }
    }
}

fn build_mono_processor(
    sample_rate: f32,
    ratio1: f32,
    ratio2: f32,
    mix: f32,
    delay1: f32,
    delay2: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_RATIO1, ratio1),
            (PORT_RATIO2, ratio2),
            (PORT_MIX, mix),
            (PORT_DELAY1, delay1),
            (PORT_DELAY2, delay2),
            (PORT_CUTOFF, 2250.0),
            (PORT_BLUR, 0.25),
        ],
    )
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let voice1 = required_f32(params, "voice1").map_err(anyhow::Error::msg)?;
    let voice2 = required_f32(params, "voice2").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;
    let delay1 = required_f32(params, "delay1").map_err(anyhow::Error::msg)?;
    let delay2 = required_f32(params, "delay2").map_err(anyhow::Error::msg)?;

    let ratio1 = semitones_to_ratio(voice1);
    let ratio2 = semitones_to_ratio(voice2);

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, ratio1, ratio2, mix, delay1, delay2)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, ratio1, ratio2, mix, delay1, delay2)?;
            let right = build_mono_processor(sample_rate, ratio1, ratio2, mix, delay1, delay2)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 {
                left,
                right,
                left_buf: Box::new([0.0; MAX_BLOCK]),
                right_buf: Box::new([0.0; MAX_BLOCK]),
            })))
        }
    }
}

pub const MODEL_DEFINITION: PitchModelDefinition = PitchModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: PitchBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
