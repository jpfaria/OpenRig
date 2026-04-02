use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_floaty";
pub const DISPLAY_NAME: &str = "Floaty";
const BRAND: &str = "remaincalm";

const PLUGIN_URI: &str = "http://remaincalm.org/plugins/floaty";
const PLUGIN_DIR: &str = "floaty.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "floaty_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "floaty_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "floaty_dsp.dll";

// LV2 port indices (from TTL) — mono in, stereo out
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT_L: usize = 1;
const PORT_DELAY: usize = 2;
const PORT_MIX: usize = 3;
const PORT_FEEDBACK: usize = 4;
const PORT_WARP: usize = 5;
const PORT_FILTER: usize = 6;
const PORT_RATE: usize = 7;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("delay", "Time", None, Some(280.0), 10.0, 653.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("mix", "Mix", None, Some(42.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("feedback", "Feedback", None, Some(20.0), 0.0, 60.0, 1.0, ParameterUnit::Percent),
            float_parameter("warp", "Warp", None, Some(60.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("filter", "Tone", None, Some(19.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("rate", "Rate", None, Some(1.0), -2.0, 2.0, 0.1, ParameterUnit::None),
        ],
    }
}

struct DualMonoFloaty {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoFloaty {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, delay: f32, mix: f32, feedback: f32, warp: f32, filter: f32, rate: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT_L],
        &[(PORT_DELAY, delay), (PORT_MIX, mix), (PORT_FEEDBACK, feedback), (PORT_WARP, warp), (PORT_FILTER, filter), (PORT_RATE, rate)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let delay = required_f32(params, "delay").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)?;
    let warp = required_f32(params, "warp").map_err(anyhow::Error::msg)?;
    let filter = required_f32(params, "filter").map_err(anyhow::Error::msg)?;
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, delay, mix, feedback, warp, filter, rate)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, delay, mix, feedback, warp, filter, rate)?;
            let right = build_mono_processor(sample_rate, delay, mix, feedback, warp, filter, rate)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoFloaty { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
