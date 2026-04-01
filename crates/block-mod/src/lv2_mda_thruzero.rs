use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_thruzero";
pub const DISPLAY_NAME: &str = "MDA ThruZero";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/mda/ThruZero";
const PLUGIN_DIR: &str = "mod-mda-ThruZero.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ThruZero.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ThruZero.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ThruZero.dll";

// LV2 port indices (from TTL)
const PORT_RATE: usize = 0;
const PORT_DEPTH: usize = 1;
const PORT_MIX: usize = 2;
const PORT_FEEDBACK: usize = 3;
const PORT_DEPTH_MOD: usize = 4;
const PORT_LEFT_IN: usize = 5;
const PORT_RIGHT_IN: usize = 6;
const PORT_LEFT_OUT: usize = 7;
const PORT_RIGHT_OUT: usize = 8;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "rate",
                "Rate",
                None,
                Some(0.3),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(0.43),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                None,
                Some(0.47),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
            float_parameter(
                "feedback",
                "Feedback",
                None,
                Some(0.3),
                0.0,
                1.0,
                0.01,
                ParameterUnit::None,
            ),
        ],
    }
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!(
        "LV2 binary '{}' not found in '{}'",
        PLUGIN_BINARY,
        lv2::default_lv2_lib_dir()
    )
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir
            .as_ref()
            .map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
        Some(std::path::PathBuf::from("plugins").join(PLUGIN_DIR)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("LV2 bundle '{}' not found in plugins/", PLUGIN_DIR)
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)?;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_LEFT_IN],
                &[PORT_LEFT_OUT],
                &[
                    (PORT_RATE, rate),
                    (PORT_DEPTH, depth),
                    (PORT_MIX, mix),
                    (PORT_FEEDBACK, feedback),
                    (PORT_DEPTH_MOD, 1.0),
                ],
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
                    (PORT_RATE, rate),
                    (PORT_DEPTH, depth),
                    (PORT_MIX, mix),
                    (PORT_FEEDBACK, feedback),
                    (PORT_DEPTH_MOD, 1.0),
                ],
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ModBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
