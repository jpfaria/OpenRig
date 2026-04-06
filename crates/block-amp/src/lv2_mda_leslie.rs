use crate::registry::{AmpBackendKind, AmpModelDefinition};
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_mda_leslie";
pub const DISPLAY_NAME: &str = "MDA Leslie";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mda/Leslie";
const PLUGIN_DIR: &str = "mod-mda-Leslie";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Leslie.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Leslie.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Leslie.dll";

// LV2 port indices (from TTL)
const PORT_MODE: usize = 0;
const PORT_LO_WIDTH: usize = 1;
const PORT_LO_THROB: usize = 2;
const PORT_HI_WIDTH: usize = 3;
const PORT_HI_DEPTH: usize = 4;
const PORT_HI_THROB: usize = 5;
const PORT_X_OVER: usize = 6;
const PORT_OUTPUT: usize = 7;
const PORT_SPEED: usize = 8;
const PORT_AUDIO_IN_L: usize = 9;
const PORT_AUDIO_IN_R: usize = 10;
const PORT_AUDIO_OUT_L: usize = 11;
const PORT_AUDIO_OUT_R: usize = 12;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_AMP.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "lo_width", "Lo Width", None, Some(50.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "lo_throb", "Lo Throb", None, Some(60.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "hi_width", "Hi Width", None, Some(70.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "hi_depth", "Hi Depth", None, Some(70.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "hi_throb", "Hi Throb", None, Some(70.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
            float_parameter(
                "speed", "Speed", None, Some(50.0),
                0.0, 100.0, 1.0, ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate(params: &ParameterSet) -> Result<()> {
    let _ = params;
    Ok(())
}

fn asset_summary(params: &ParameterSet) -> Result<String> {
    let _ = params;
    Ok(String::new())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    // MDA lvz wrapper expects all params normalized 0-1
    let lo_width = required_f32(params, "lo_width").map_err(anyhow::Error::msg)? / 100.0;
    let lo_throb = required_f32(params, "lo_throb").map_err(anyhow::Error::msg)? / 100.0;
    let hi_width = required_f32(params, "hi_width").map_err(anyhow::Error::msg)? / 100.0;
    let hi_depth = required_f32(params, "hi_depth").map_err(anyhow::Error::msg)? / 100.0;
    let hi_throb = required_f32(params, "hi_throb").map_err(anyhow::Error::msg)? / 100.0;
    let speed = required_f32(params, "speed").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_MODE, 0.5_f32),
        (PORT_LO_WIDTH, lo_width),
        (PORT_LO_THROB, lo_throb),
        (PORT_HI_WIDTH, hi_width),
        (PORT_HI_DEPTH, hi_depth),
        (PORT_HI_THROB, hi_throb),
        (PORT_X_OVER, 0.46_f32),
        (PORT_OUTPUT, 0.5_f32),
        (PORT_SPEED, speed),
    ];

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
        &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
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

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: AmpModelDefinition = AmpModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: AmpBackendKind::Lv2,
    schema,
    validate,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_ACOUSTIC_BASS,
    knob_layout: &[],
};
