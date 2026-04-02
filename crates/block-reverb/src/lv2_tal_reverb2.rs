use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tal_reverb2";
pub const DISPLAY_NAME: &str = "TAL Reverb II";
const BRAND: &str = "tal";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tal-reverb-2";
const PLUGIN_DIR: &str = "mod-tal-Reverb-2.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "TAL-Reverb-2.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "TAL-Reverb-2.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "TAL-Reverb-2.dll";

// LV2 port indices (from TTL)
const PORT_FREEWHEEL: usize = 0;
const PORT_AUDIO_IN_L: usize = 1;
const PORT_AUDIO_IN_R: usize = 2;
const PORT_AUDIO_OUT_L: usize = 3;
const PORT_AUDIO_OUT_R: usize = 4;
const PORT_DRY: usize = 5;
const PORT_WET: usize = 6;
const PORT_ROOM_SIZE: usize = 7;
const PORT_PRE_DELAY: usize = 8;
const PORT_LOW_SHELF_FREQ: usize = 9;
const PORT_HIGH_SHELF_FREQ: usize = 10;
const PORT_STEREO_WIDTH: usize = 11;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("dry", "Dry", None, Some(0.0), -96.0, 26.0, 0.5, ParameterUnit::Decibels),
            float_parameter("wet", "Wet", None, Some(-24.0), -96.0, 26.0, 0.5, ParameterUnit::Decibels),
            float_parameter("room_size", "Room Size", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("pre_delay", "Pre Delay", None, Some(37.0), 0.0, 1000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("low_shelf_freq", "Low Freq", None, Some(260.0), 100.0, 10000.0, 10.0, ParameterUnit::Hertz),
            float_parameter("high_shelf_freq", "High Freq", None, Some(3040.0), 100.0, 10000.0, 10.0, ParameterUnit::Hertz),
            float_parameter("stereo_width", "Stereo Width", None, Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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
    let dry = required_f32(params, "dry").map_err(anyhow::Error::msg)?;
    let wet = required_f32(params, "wet").map_err(anyhow::Error::msg)?;
    let room_size = required_f32(params, "room_size").map_err(anyhow::Error::msg)? / 100.0;
    let pre_delay = required_f32(params, "pre_delay").map_err(anyhow::Error::msg)?;
    let low_shelf_freq = required_f32(params, "low_shelf_freq").map_err(anyhow::Error::msg)?;
    let high_shelf_freq = required_f32(params, "high_shelf_freq").map_err(anyhow::Error::msg)?;
    let stereo_width = required_f32(params, "stereo_width").map_err(anyhow::Error::msg)? / 100.0;

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let control_ports = &[
        (PORT_FREEWHEEL, 0.0),
        (PORT_DRY, dry),
        (PORT_WET, wet),
        (PORT_ROOM_SIZE, room_size),
        (PORT_PRE_DELAY, pre_delay),
        (PORT_LOW_SHELF_FREQ, low_shelf_freq),
        (PORT_HIGH_SHELF_FREQ, high_shelf_freq),
        (PORT_STEREO_WIDTH, stereo_width),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor_with_extras(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L], &[PORT_AUDIO_OUT_L], control_ports,
                &[PORT_AUDIO_IN_R, PORT_AUDIO_OUT_R],
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let processor = lv2::build_stereo_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
                control_ports,
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
