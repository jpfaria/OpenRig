use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_tap_echo";
pub const DISPLAY_NAME: &str = "TAP Stereo Echo";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/echo";
const PLUGIN_DIR: &str = "tap-echo";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_echo.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_echo.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_echo.dll";

// LV2 port indices (from TTL) — stereo in/out
const PORT_L_DELAY: usize = 0;
const PORT_L_FEEDBACK: usize = 1;
const PORT_R_HAAS_DELAY: usize = 2;
const PORT_R_HAAS_FEEDBACK: usize = 3;
const PORT_L_ECHO_LEVEL: usize = 4;
const PORT_R_ECHO_LEVEL: usize = 5;
const PORT_DRY_LEVEL: usize = 6;
const PORT_CROSS_MODE: usize = 7;
const PORT_HAAS: usize = 8;
const PORT_SWAP: usize = 9;
const PORT_INPUT_L: usize = 10;
const PORT_OUTPUT_L: usize = 11;
const PORT_INPUT_R: usize = 12;
const PORT_OUTPUT_R: usize = 13;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("l_delay", "L Delay", None, Some(300.0), 0.0, 2000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("l_feedback", "L Feedback", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("r_haas_delay", "R Delay", None, Some(300.0), 0.0, 2000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("r_haas_feedback", "R Feedback", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("l_echo_level", "L Echo Level", None, Some(-4.0), -70.0, 10.0, 0.5, ParameterUnit::Decibels),
            float_parameter("r_echo_level", "R Echo Level", None, Some(-4.0), -70.0, 10.0, 0.5, ParameterUnit::Decibels),
            float_parameter("dry_level", "Dry Level", None, Some(-4.0), -70.0, 10.0, 0.5, ParameterUnit::Decibels),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let l_delay = required_f32(params, "l_delay").map_err(anyhow::Error::msg)?;
    let l_feedback = required_f32(params, "l_feedback").map_err(anyhow::Error::msg)?;
    let r_haas_delay = required_f32(params, "r_haas_delay").map_err(anyhow::Error::msg)?;
    let r_haas_feedback = required_f32(params, "r_haas_feedback").map_err(anyhow::Error::msg)?;
    let l_echo_level = required_f32(params, "l_echo_level").map_err(anyhow::Error::msg)?;
    let r_echo_level = required_f32(params, "r_echo_level").map_err(anyhow::Error::msg)?;
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_L_DELAY, l_delay), (PORT_L_FEEDBACK, l_feedback),
        (PORT_R_HAAS_DELAY, r_haas_delay), (PORT_R_HAAS_FEEDBACK, r_haas_feedback),
        (PORT_L_ECHO_LEVEL, l_echo_level), (PORT_R_ECHO_LEVEL, r_echo_level),
        (PORT_DRY_LEVEL, dry_level),
        // Toggled ports — fixed defaults
        (PORT_CROSS_MODE, 1.0), (PORT_HAAS, 0.0), (PORT_SWAP, 0.0),
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
