use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_paranoia";
pub const DISPLAY_NAME: &str = "Paranoia";
const BRAND: &str = "remaincalm";

const PLUGIN_URI: &str = "http://remaincalm.org/plugins/paranoia";
const PLUGIN_DIR: &str = "paranoia.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "paranoia_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "paranoia_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "paranoia_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_LEVEL: usize = 2;
const PORT_CRUSH: usize = 3;
const PORT_MANGLE: usize = 4;
const PORT_FILTER: usize = 5;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "crush",
                "Crush",
                None,
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "mangle",
                "Mangle",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "filter",
                "Filter",
                None,
                Some(40.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "crush").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "mangle").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "filter").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "level").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("lv2='{}'", MODEL_ID))
}

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

fn build_mono_processor(
    sample_rate: f32,
    crush: f32,
    mangle: f32,
    filter: f32,
    level: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_CRUSH, crush),
            (PORT_MANGLE, mangle),
            (PORT_FILTER, filter),
            (PORT_LEVEL, level),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let crush = required_f32(params, "crush").map_err(anyhow::Error::msg)?;
    // Mangle: 0-100% maps to 0-16
    let mangle = required_f32(params, "mangle").map_err(anyhow::Error::msg)? / 100.0 * 16.0;
    let filter = required_f32(params, "filter").map_err(anyhow::Error::msg)?;
    // Level: 0-100% maps to -24..+6 dB
    let level_pct = required_f32(params, "level").map_err(anyhow::Error::msg)?;
    let level = -24.0 + (level_pct / 100.0) * 30.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, crush, mangle, filter, level)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, crush, mangle, filter, level)?;
            let right = build_mono_processor(sample_rate, crush, mangle, filter, level)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: GainModelDefinition = GainModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: GainBackendKind::Lv2,
    schema,
    validate: validate_params,
    asset_summary,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
