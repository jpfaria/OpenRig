use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_reflector";
pub const DISPLAY_NAME: &str = "TAP Reflector";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/reflector";
const PLUGIN_DIR: &str = "tap-reflector";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_reflector.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_reflector.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_reflector.dll";

// LV2 port indices (from TTL) — mono in, mono out
const PORT_FRAGMENT: usize = 0;
const PORT_DRY: usize = 1;
const PORT_WET: usize = 2;
const PORT_AUDIO_IN: usize = 3;
const PORT_AUDIO_OUT: usize = 4;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("fragment", "Fragment", None, Some(1000.0), 100.0, 1600.0, 10.0, ParameterUnit::Milliseconds),
            float_parameter("dry", "Dry", None, Some(0.0), -90.0, 20.0, 0.5, ParameterUnit::Decibels),
            float_parameter("wet", "Wet", None, Some(0.0), -90.0, 20.0, 0.5, ParameterUnit::Decibels),
        ],
    }
}

struct DualMonoReflector {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoReflector {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, fragment: f32, dry: f32, wet: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_FRAGMENT, fragment), (PORT_DRY, dry), (PORT_WET, wet)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let fragment = required_f32(params, "fragment").map_err(anyhow::Error::msg)?;
    let dry = required_f32(params, "dry").map_err(anyhow::Error::msg)?;
    let wet = required_f32(params, "wet").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, fragment, dry, wet)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, fragment, dry, wet)?;
            let right = build_mono_processor(sample_rate, fragment, dry, wet)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoReflector { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
