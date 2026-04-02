use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_caps_scape";
pub const DISPLAY_NAME: &str = "CAPS Scape";
const BRAND: &str = "caps";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/Scape";
const PLUGIN_DIR: &str = "mod-caps-Scape.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Scape.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Scape.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Scape.dll";

// LV2 port indices (from TTL) — mono in, stereo out
const PORT_BPM: usize = 0;
const PORT_DIVIDER: usize = 1;
const PORT_FEEDBACK: usize = 2;
const PORT_DRY: usize = 3;
const PORT_BLEND: usize = 4;
const PORT_TUNE: usize = 5;
const PORT_AUDIO_IN: usize = 6;
const PORT_AUDIO_OUT_L: usize = 7;
const PORT_AUDIO_OUT_R: usize = 8;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("bpm", "BPM", None, Some(100.0), 30.0, 164.0, 1.0, ParameterUnit::None),
            float_parameter("feedback", "Feedback", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("dry", "Dry", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("blend", "Blend", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

struct DualMonoScape {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoScape {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, bpm: f32, feedback: f32, dry: f32, blend: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT_L],
        &[(PORT_BPM, bpm), (PORT_DIVIDER, 2.0), (PORT_FEEDBACK, feedback), (PORT_DRY, dry), (PORT_BLEND, blend), (PORT_TUNE, 440.0)],
        &[PORT_AUDIO_OUT_R],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let bpm = required_f32(params, "bpm").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)? / 100.0;
    let dry = required_f32(params, "dry").map_err(anyhow::Error::msg)? / 100.0;
    let blend = required_f32(params, "blend").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, bpm, feedback, dry, blend)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, bpm, feedback, dry, blend)?;
            let right = build_mono_processor(sample_rate, bpm, feedback, dry, blend)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoScape { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
