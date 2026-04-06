use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_gx_superfuzz";
pub const DISPLAY_NAME: &str = "Super Fuzz";
const BRAND: &str = "guitarix";

const PLUGIN_URI: &str = "http://guitarix.sourceforge.net/plugins/gx_sfp_#_sfp_";
const PLUGIN_DIR: &str = "GxSuperFuzz";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "gx_sfp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "gx_sfp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "gx_sfp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_OUT: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_BALANCE: usize = 2;
const PORT_EXPANDER: usize = 3;
const PORT_TONE: usize = 4;
const PORT_TRIM: usize = 5;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "balance",
                "Balance",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "expander",
                "Expander",
                None,
                Some(50.0),
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
            float_parameter(
                "trim",
                "Trim",
                None,
                Some(0.25),
                0.0,
                0.49,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "balance").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "expander").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "trim").map_err(anyhow::Error::msg)?;
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
    balance: f32,
    expander: f32,
    tone: f32,
    trim: f32,
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
            (PORT_BALANCE, balance),
            (PORT_EXPANDER, expander),
            (PORT_TONE, tone),
            (PORT_TRIM, trim),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let balance = required_f32(params, "balance").map_err(anyhow::Error::msg)? / 100.0;
    let expander = required_f32(params, "expander").map_err(anyhow::Error::msg)? / 100.0;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)? / 100.0;
    let trim = required_f32(params, "trim").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, balance, expander, tone, trim)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, balance, expander, tone, trim)?;
            let right = build_mono_processor(sample_rate, balance, expander, tone, trim)?;
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
