use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_modulay";
pub const DISPLAY_NAME: &str = "Modulay";
const BRAND: &str = "shiro";

const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/modulay";
const PLUGIN_DIR: &str = "Modulay";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Modulay_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Modulay_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Modulay_dsp.dll";

// LV2 port indices (from TTL) — mono in, mono out
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_REPEATS: usize = 2;
const PORT_MIX: usize = 3;
const PORT_RATE: usize = 4;
const PORT_DEPTH: usize = 5;
const PORT_TIME: usize = 6;
const PORT_MORPH: usize = 7;
const PORT_TONE: usize = 8;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("repeats", "Repeats", None, Some(75.0), 0.0, 110.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("rate", "Rate", None, Some(2.0), 0.1, 5.0, 0.1, ParameterUnit::Hertz),
            float_parameter("depth", "Depth", None, Some(1.0), 0.1, 3.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("time", "Time", None, Some(500.0), 20.0, 1000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("morph", "Morph", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tone", "Tone", None, Some(3000.0), 500.0, 6000.0, 10.0, ParameterUnit::Hertz),
        ],
    }
}

struct DualMonoModulay {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoModulay {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(
    sample_rate: f32, repeats: f32, mix: f32, rate: f32, depth: f32, time: f32, morph: f32, tone: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[
            (PORT_REPEATS, repeats), (PORT_MIX, mix), (PORT_RATE, rate),
            (PORT_DEPTH, depth), (PORT_TIME, time), (PORT_MORPH, morph), (PORT_TONE, tone),
        ],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let repeats = required_f32(params, "repeats").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let time = required_f32(params, "time").map_err(anyhow::Error::msg)?;
    let morph = required_f32(params, "morph").map_err(anyhow::Error::msg)?;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, repeats, mix, rate, depth, time, morph, tone)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, repeats, mix, rate, depth, time, morph, tone)?;
            let right = build_mono_processor(sample_rate, repeats, mix, rate, depth, time, morph, tone)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoModulay { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DelayBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
