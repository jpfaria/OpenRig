use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_satma";
pub const DISPLAY_NAME: &str = "Satma";
const BRAND: &str = "artyfx";

const PLUGIN_URI: &str = "http://www.openavproductions.com/artyfx#satma";
const PLUGIN_DIR: &str = "artyfx";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "artyfx.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "artyfx.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "artyfx.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_DISTORT: usize = 2;
const PORT_TONE: usize = 3;
const PORT_ACTIVE: usize = 4;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "distort",
                "Distort",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "tone",
                "Tone",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "distort").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
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
    distort: f32,
    tone: f32,
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
            (PORT_DISTORT, distort),
            (PORT_TONE, tone),
            (PORT_ACTIVE, 1.0),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let distort = required_f32(params, "distort").map_err(anyhow::Error::msg)? / 100.0;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)? / 100.0;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, distort, tone)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, distort, tone)?;
            let right = build_mono_processor(sample_rate, distort, tone)?;
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
