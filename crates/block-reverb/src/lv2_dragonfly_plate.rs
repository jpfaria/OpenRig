use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_dragonfly_plate";
pub const DISPLAY_NAME: &str = "Dragonfly Plate Reverb";
const BRAND: &str = "dragonfly";

const PLUGIN_URI: &str = "urn:dragonfly:plate";
const PLUGIN_DIR: &str = "DragonflyPlateReverb.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "DragonflyPlateReverb_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "DragonflyPlateReverb_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "DragonflyPlateReverb_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_ATOM_IN: usize = 4;
const PORT_ATOM_OUT: usize = 5;
const PORT_DRY_LEVEL: usize = 6;
const PORT_WET_LEVEL: usize = 7;
const PORT_ALGORITHM: usize = 8;
const PORT_WIDTH: usize = 9;
const PORT_PREDELAY: usize = 10;
const PORT_DECAY: usize = 11;
const PORT_LOW_CUT: usize = 12;
const PORT_HIGH_CUT: usize = 13;
const PORT_DAMPEN: usize = 14;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("dry_level", "Dry Level", None, Some(80.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("wet_level", "Wet Level", None, Some(20.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            enum_parameter("algorithm", "Algorithm", None, Some("nested_allpass"), &[
                ("nested_allpass", "Nested Allpass"),
                ("allpass", "Allpass"),
                ("freeverb", "Freeverb"),
            ]),
            float_parameter("width", "Width", None, Some(100.0), 50.0, 150.0, 1.0, ParameterUnit::Percent),
            float_parameter("predelay", "Predelay", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("decay", "Decay", None, Some(0.4), 0.1, 10.0, 0.1, ParameterUnit::None),
            float_parameter("low_cut", "Low Cut", None, Some(200.0), 0.0, 200.0, 1.0, ParameterUnit::Hertz),
            float_parameter("high_cut", "High Cut", None, Some(16000.0), 1000.0, 16000.0, 100.0, ParameterUnit::Hertz),
            float_parameter("dampen", "Dampen", None, Some(13000.0), 1000.0, 16000.0, 100.0, ParameterUnit::Hertz),
        ],
    }
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
        Some(std::path::PathBuf::from(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("LV2 binary '{}' not found in '{}'", PLUGIN_BINARY, lv2::default_lv2_lib_dir())
}

fn resolve_bundle_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../plugins").join(PLUGIN_DIR)),
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
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;
    let algorithm_str = required_string(params, "algorithm").map_err(anyhow::Error::msg)?;
    let algorithm: f32 = match algorithm_str.as_str() {
        "allpass" => 1.0,
        "freeverb" => 2.0,
        _ => 0.0, // nested_allpass
    };
    let width = required_f32(params, "width").map_err(anyhow::Error::msg)?;
    let predelay = required_f32(params, "predelay").map_err(anyhow::Error::msg)?;
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)?;
    let low_cut = required_f32(params, "low_cut").map_err(anyhow::Error::msg)?;
    let high_cut = required_f32(params, "high_cut").map_err(anyhow::Error::msg)?;
    let dampen = required_f32(params, "dampen").map_err(anyhow::Error::msg)?;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let control_ports = &[
        (PORT_DRY_LEVEL, dry_level),
        (PORT_WET_LEVEL, wet_level),
        (PORT_ALGORITHM, algorithm),
        (PORT_WIDTH, width),
        (PORT_PREDELAY, predelay),
        (PORT_DECAY, decay),
        (PORT_LOW_CUT, low_cut),
        (PORT_HIGH_CUT, high_cut),
        (PORT_DAMPEN, dampen),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor_full(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L], &[PORT_AUDIO_OUT_L], control_ports,
                &[PORT_ATOM_IN, PORT_ATOM_OUT], &[PORT_AUDIO_IN_R, PORT_AUDIO_OUT_R],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor_with_atoms(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
                control_ports, &[PORT_ATOM_IN, PORT_ATOM_OUT],
            )?;
            Ok(BlockProcessor::Stereo(Box::new(processor)))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
}

pub const MODEL_DEFINITION: ReverbModelDefinition = ReverbModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ReverbBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
