use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_detune";
pub const DISPLAY_NAME: &str = "MDA Detune";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mda/Detune";
const PLUGIN_DIR: &str = "mod-mda-Detune.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Detune.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Detune.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Detune.dll";

// LV2 port indices (from Detune.ttl)
const PORT_DETUNE: usize = 0;
const PORT_MIX: usize = 1;
const PORT_LEVEL: usize = 2;
const PORT_LATENCY: usize = 3;
const PORT_LEFT_IN: usize = 4;
const PORT_RIGHT_IN: usize = 5;
const PORT_LEFT_OUT: usize = 6;
const PORT_RIGHT_OUT: usize = 7;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "detune",
                "Detune",
                Some("Pitch"),
                Some(2.4),
                0.0,
                300.0,
                0.1,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Control"),
                Some(50.0),
                0.0,
                99.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                Some("Control"),
                Some(0.0),
                -20.0,
                20.0,
                0.1,
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

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let detune = required_f32(params, "detune").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let level = required_f32(params, "level").map_err(anyhow::Error::msg)?;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let control_ports = &[
        (PORT_DETUNE, detune),
        (PORT_MIX, mix),
        (PORT_LEVEL, level),
        (PORT_LATENCY, 0.0),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor(
                &lib_path,
                PLUGIN_URI,
                sample_rate as f64,
                &bundle_path,
                &[PORT_LEFT_IN],
                &[PORT_LEFT_OUT],
                control_ports,
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
                control_ports,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

pub const MODEL_DEFINITION: PitchModelDefinition = PitchModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: PitchBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
