use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_degrade";
pub const DISPLAY_NAME: &str = "MDA Degrade";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/Degrade";
const PLUGIN_DIR: &str = "mod-mda-Degrade";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Degrade.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Degrade.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Degrade.dll";

// LV2 port indices — binary has 6 control params, NOT 8.
// The internal effect (mdaDegrade) has 6 params: clip, bits, rate, postfilt, nonlin, level.
// Audio ports follow immediately after.
const PORT_HEADROOM: usize = 0;
const PORT_QUANT: usize = 1;
const PORT_RATE: usize = 2;
const PORT_POST_FILTER: usize = 3;
const PORT_NON_LIN: usize = 4;
const PORT_OUTPUT: usize = 5;
const PORT_LEFT_IN: usize = 6;
const PORT_RIGHT_IN: usize = 7;
const PORT_LEFT_OUT: usize = 8;
const PORT_RIGHT_OUT: usize = 9;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "headroom",
                "Headroom",
                None,
                Some(80.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "quant",
                "Quant",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "rate",
                "Rate",
                None,
                Some(65.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "post_filter",
                "Post Filter",
                None,
                Some(90.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "non_lin",
                "Non-Lin",
                None,
                Some(58.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "output",
                "Output",
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
    let _ = required_f32(params, "headroom").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "quant").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "rate").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "post_filter").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "non_lin").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "output").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("lv2='{}'", MODEL_ID))
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    // LVZ wrapper uses 0-1 normalized params internally
    let headroom = required_f32(params, "headroom").map_err(anyhow::Error::msg)? / 100.0;
    let quant = required_f32(params, "quant").map_err(anyhow::Error::msg)? / 100.0;
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)? / 100.0;
    let post_filter = required_f32(params, "post_filter").map_err(anyhow::Error::msg)? / 100.0;
    let non_lin = required_f32(params, "non_lin").map_err(anyhow::Error::msg)? / 100.0;
    let output = required_f32(params, "output").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_HEADROOM, headroom),
        (PORT_QUANT, quant),
        (PORT_RATE, rate),
        (PORT_POST_FILTER, post_filter),
        (PORT_NON_LIN, non_lin),
        (PORT_OUTPUT, output),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor_with_extras(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_LEFT_IN], &[PORT_LEFT_OUT], control_ports,
                &[PORT_RIGHT_IN, PORT_RIGHT_OUT],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_LEFT_IN, PORT_RIGHT_IN], &[PORT_LEFT_OUT, PORT_RIGHT_OUT],
                control_ports,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
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
