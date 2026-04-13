use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_shiroverb";
pub const DISPLAY_NAME: &str = "Shiroverb";
const BRAND: &str = "shiro";

const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/shiroverb";
const PLUGIN_DIR: &str = "Shiroverb";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Shiroverb_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Shiroverb_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Shiroverb_dsp.dll";

// LV2 port indices (from TTL) — mono in, stereo out
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT_L: usize = 1;
const PORT_AUDIO_OUT_R: usize = 2;
const PORT_SHIMMER: usize = 3;
const PORT_DECAY: usize = 4;
const PORT_DAMPING: usize = 5;
const PORT_MIX: usize = 6;
const PORT_RATIO: usize = 7;
const PORT_ROOMSIZE: usize = 8;
const PORT_BANDWIDTH: usize = 9;
const PORT_TONE: usize = 10;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("shimmer", "Shimmer", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("decay", "Decay", None, Some(50.0), 1.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("ratio", "Ratio", None, Some(2.0), 0.5, 2.0, 0.1, ParameterUnit::None),
            float_parameter("roomsize", "Room Size", None, Some(150.0), 1.0, 300.0, 1.0, ParameterUnit::None),
            float_parameter("bandwidth", "Bandwidth", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", None, Some(1500.0), 500.0, 6000.0, 10.0, ParameterUnit::Hertz),
        ],
    }
}

// Shiroverb is mono-in, stereo-out. For DualMono stereo we use two instances.
struct DualMonoShiroverb {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoShiroverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

fn build_mono_processor(
    sample_rate: f32,
    shimmer: f32,
    decay: f32,
    damping: f32,
    mix: f32,
    ratio: f32,
    roomsize: f32,
    bandwidth: f32,
    tone: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    lv2::build_lv2_processor_with_extras(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT_L],
        &[
            (PORT_SHIMMER, shimmer),
            (PORT_DECAY, decay),
            (PORT_DAMPING, damping),
            (PORT_MIX, mix),
            (PORT_RATIO, ratio),
            (PORT_ROOMSIZE, roomsize),
            (PORT_BANDWIDTH, bandwidth),
            (PORT_TONE, tone),
        ],
        &[PORT_AUDIO_OUT_R],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let shimmer = required_f32(params, "shimmer").map_err(anyhow::Error::msg)?;
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)?;
    let damping = required_f32(params, "damping").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let ratio = required_f32(params, "ratio").map_err(anyhow::Error::msg)?;
    let roomsize = required_f32(params, "roomsize").map_err(anyhow::Error::msg)?;
    let bandwidth = required_f32(params, "bandwidth").map_err(anyhow::Error::msg)?;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, shimmer, decay, damping, mix, ratio, roomsize, bandwidth, tone)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, shimmer, decay, damping, mix, ratio, roomsize, bandwidth, tone)?;
            let right = build_mono_processor(sample_rate, shimmer, decay, damping, mix, ratio, roomsize, bandwidth, tone)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoShiroverb { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
