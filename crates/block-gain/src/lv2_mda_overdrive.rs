use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_overdrive";
pub const DISPLAY_NAME: &str = "MDA Overdrive";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/Overdrive";
const PLUGIN_DIR: &str = "mod-mda-Overdrive";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Overdrive.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Overdrive.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Overdrive.dll";

// LV2 port indices (from TTL)
const PORT_DRIVE: usize = 0;
const PORT_MUFFLE: usize = 1;
const PORT_OUTPUT: usize = 2;
const PORT_LEFT_IN: usize = 3;
const PORT_RIGHT_IN: usize = 4;
const PORT_LEFT_OUT: usize = 5;
const PORT_RIGHT_OUT: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "drive",
                "Drive",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "muffle",
                "Muffle",
                None,
                Some(0.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                None,
                Some(60.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "muffle").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "level").map_err(anyhow::Error::msg)?;
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
    // lvz wrapper normalizes all params to 0-1
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)? / 100.0;
    let muffle = required_f32(params, "muffle").map_err(anyhow::Error::msg)? / 100.0;
    let level_pct = required_f32(params, "level").map_err(anyhow::Error::msg)?;
    let output = level_pct / 100.0;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor_with_extras(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_LEFT_IN], &[PORT_LEFT_OUT],
                &[(PORT_DRIVE, drive), (PORT_MUFFLE, muffle), (PORT_OUTPUT, output)],
                &[PORT_RIGHT_IN, PORT_RIGHT_OUT],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_LEFT_IN, PORT_RIGHT_IN],
                &[PORT_LEFT_OUT, PORT_RIGHT_OUT],
                &[
                    (PORT_DRIVE, drive),
                    (PORT_MUFFLE, muffle),
                    (PORT_OUTPUT, output),
                ],
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
