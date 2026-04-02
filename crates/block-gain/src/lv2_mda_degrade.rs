use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
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

// LV2 port indices (from TTL)
const PORT_HEADROOM: usize = 0;
const PORT_QUANT: usize = 1;
const PORT_RATE: usize = 2;
const PORT_INTEGRATOR: usize = 3;
const PORT_POST_FILTER: usize = 4;
const PORT_NON_LIN: usize = 5;
const PORT_EVEN_ODD: usize = 6;
const PORT_OUTPUT: usize = 7;
const PORT_LEFT_IN: usize = 8;
const PORT_RIGHT_IN: usize = 9;
const PORT_LEFT_OUT: usize = 10;
const PORT_RIGHT_OUT: usize = 11;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
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
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "non_lin",
                "Non-Lin",
                None,
                Some(16.0),
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
            enum_parameter(
                "integrator",
                "Integrator",
                None,
                Some("off"),
                &[("off", "Off"), ("on", "On")],
            ),
            enum_parameter(
                "even_odd",
                "Even/Odd",
                None,
                Some("odd"),
                &[("even", "Even"), ("odd", "Odd")],
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "headroom").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "quant").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "rate").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "non_lin").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "output").map_err(anyhow::Error::msg)?;
    let _ = required_string(params, "integrator").map_err(anyhow::Error::msg)?;
    let _ = required_string(params, "even_odd").map_err(anyhow::Error::msg)?;
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
    // Headroom: 0-100% maps to -30..0 dB
    let headroom_pct = required_f32(params, "headroom").map_err(anyhow::Error::msg)?;
    let headroom = -30.0 + (headroom_pct / 100.0) * 30.0;
    // Quant: 0-100% maps to 4-16 bits
    let quant_pct = required_f32(params, "quant").map_err(anyhow::Error::msg)?;
    let quant = 4.0 + (quant_pct / 100.0) * 12.0;
    // Rate: 0-100% maps to 4800-48000 Hz (logarithmic)
    let rate_pct = required_f32(params, "rate").map_err(anyhow::Error::msg)?;
    let rate = 4800.0 * (48000.0_f32 / 4800.0).powf(rate_pct / 100.0);
    let non_lin = required_f32(params, "non_lin").map_err(anyhow::Error::msg)? / 100.0;
    // Output: 0-100% maps to -20..+20 dB
    let output_pct = required_f32(params, "output").map_err(anyhow::Error::msg)?;
    let output = -20.0 + (output_pct / 100.0) * 40.0;
    let integrator_str = required_string(params, "integrator").map_err(anyhow::Error::msg)?;
    let integrator: f32 = if integrator_str == "on" { 1.0 } else { 0.0 };
    let even_odd_str = required_string(params, "even_odd").map_err(anyhow::Error::msg)?;
    let even_odd: f32 = if even_odd_str == "even" { 0.0 } else { 1.0 };
    // Post filter fixed at 15000 Hz
    let post_filter = 15000.0_f32;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        (PORT_HEADROOM, headroom),
        (PORT_QUANT, quant),
        (PORT_RATE, rate),
        (PORT_INTEGRATOR, integrator),
        (PORT_POST_FILTER, post_filter),
        (PORT_NON_LIN, non_lin),
        (PORT_EVEN_ODD, even_odd),
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
