use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_caps_plate";
pub const DISPLAY_NAME: &str = "CAPS Plate";
const BRAND: &str = "caps";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/Plate";
const PLUGIN_DIR: &str = "mod-caps-Plate.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Plate.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Plate.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Plate.dll";

// LV2 port indices (from TTL) — mono in, stereo out
const PORT_BANDWIDTH: usize = 0;
const PORT_TAIL: usize = 1;
const PORT_DAMPING: usize = 2;
const PORT_BLEND: usize = 3;
const PORT_AUDIO_IN: usize = 4;
const PORT_AUDIO_OUT_L: usize = 5;
const PORT_AUDIO_OUT_R: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("bandwidth", "Bandwidth", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("tail", "Tail", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("damping", "Damping", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("blend", "Blend", None, Some(25.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

struct DualMonoCapsPlate {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoCapsPlate {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, bandwidth: f32, tail: f32, damping: f32, blend: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT_L],
        &[(PORT_BANDWIDTH, bandwidth), (PORT_TAIL, tail), (PORT_DAMPING, damping), (PORT_BLEND, blend)],
        &[PORT_AUDIO_OUT_R],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let bandwidth = required_f32(params, "bandwidth").map_err(anyhow::Error::msg)? / 100.0;
    let tail = required_f32(params, "tail").map_err(anyhow::Error::msg)? / 100.0;
    let damping = required_f32(params, "damping").map_err(anyhow::Error::msg)? / 100.0;
    let blend = required_f32(params, "blend").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, bandwidth, tail, damping, blend)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, bandwidth, tail, damping, blend)?;
            let right = build_mono_processor(sample_rate, bandwidth, tail, damping, blend)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoCapsPlate { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
