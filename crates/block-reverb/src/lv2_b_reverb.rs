use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_b_reverb";
pub const DISPLAY_NAME: &str = "B Reverb";
const BRAND: &str = "setbfree";

const PLUGIN_URI: &str = "http://gareus.org/oss/lv2/b_reverb";
const PLUGIN_DIR: &str = "b_reverb";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "b_reverb.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "b_reverb.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "b_reverb.dll";

// LV2 port indices (from TTL) — mono in, mono out
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_MIX: usize = 2;
const PORT_GAIN_IN: usize = 3;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("mix", "Mix", None, Some(30.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("gain_in", "Input Gain", None, Some(4.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

struct DualMonoBReverb {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoBReverb {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, mix: f32, gain_in: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_MIX, mix), (PORT_GAIN_IN, gain_in)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)? / 100.0;
    let gain_in = required_f32(params, "gain_in").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, mix, gain_in)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, mix, gain_in)?;
            let right = build_mono_processor(sample_rate, mix, gain_in)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoBReverb { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
