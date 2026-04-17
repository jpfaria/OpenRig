use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_tap_doubler";
pub const DISPLAY_NAME: &str = "TAP Doubler";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/doubler";
const PLUGIN_DIR: &str = "tap-doubler";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_doubler.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_doubler.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_doubler.dll";

// LV2 port indices (from TTL) — stereo in/out
const PORT_TIME_TRACKING: usize = 0;
const PORT_PITCH_TRACKING: usize = 1;
const PORT_DRY_LEVEL: usize = 2;
const PORT_DRY_LEFT: usize = 3;
const PORT_DRY_RIGHT: usize = 4;
const PORT_WET_LEVEL: usize = 5;
const PORT_WET_LEFT: usize = 6;
const PORT_WET_RIGHT: usize = 7;
const PORT_INPUT_L: usize = 8;
const PORT_INPUT_R: usize = 9;
const PORT_OUTPUT_L: usize = 10;
const PORT_OUTPUT_R: usize = 11;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("time_tracking", "Time Tracking", None, Some(0.5), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("pitch_tracking", "Pitch Tracking", None, Some(0.5), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("dry_level", "Dry Level", None, Some(0.0), -90.0, 20.0, 0.5, ParameterUnit::Decibels),
            float_parameter("wet_level", "Wet Level", None, Some(0.0), -90.0, 20.0, 0.5, ParameterUnit::Decibels),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let time_tracking = required_f32(params, "time_tracking").map_err(anyhow::Error::msg)?;
    let pitch_tracking = required_f32(params, "pitch_tracking").map_err(anyhow::Error::msg)?;
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_TIME_TRACKING, time_tracking), (PORT_PITCH_TRACKING, pitch_tracking),
        (PORT_DRY_LEVEL, dry_level),
        // Position ports — fixed defaults
        (PORT_DRY_LEFT, 0.0), (PORT_DRY_RIGHT, 1.0),
        (PORT_WET_LEVEL, wet_level),
        (PORT_WET_LEFT, 0.0), (PORT_WET_RIGHT, 1.0),
    ];

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_INPUT_L, PORT_INPUT_R], &[PORT_OUTPUT_L, PORT_OUTPUT_R],
        control_ports,
    )?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(StereoAsMono(processor)))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(processor))),
    }
}

struct StereoAsMono(lv2::StereoLv2Processor);
impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        (l + r) * 0.5
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DelayBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
