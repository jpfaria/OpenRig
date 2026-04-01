use crate::registry::ReverbModelDefinition;
use crate::ReverbBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tap_reverb";
pub const DISPLAY_NAME: &str = "TAP Reverberator";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/reverb";
const PLUGIN_DIR: &str = "tap-reverb.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_reverb.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_reverb.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_reverb.dll";

// LV2 port indices (from TTL)
const PORT_DECAY: usize = 0;
const PORT_DRY_LEVEL: usize = 1;
const PORT_WET_LEVEL: usize = 2;
const PORT_COMBS_EN: usize = 3;
const PORT_ALLPS_EN: usize = 4;
const PORT_BANDPASS_EN: usize = 5;
const PORT_STEREO_ENH: usize = 6;
const PORT_MODE: usize = 7;
const PORT_AUDIO_IN_L: usize = 8;
const PORT_AUDIO_OUT_L: usize = 9;
const PORT_AUDIO_IN_R: usize = 10;
const PORT_AUDIO_OUT_R: usize = 11;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_REVERB.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("decay", "Decay", None, Some(2800.0), 0.0, 10000.0, 10.0, ParameterUnit::Milliseconds),
            float_parameter("dry_level", "Dry Level", None, Some(-4.0), -70.0, 10.0, 0.5, ParameterUnit::Decibels),
            float_parameter("wet_level", "Wet Level", None, Some(-12.0), -70.0, 10.0, 0.5, ParameterUnit::Decibels),
            enum_parameter("mode", "Reverb Type", None, Some("0"), &[
                ("0", "AfterBurn"),
                ("1", "AfterBurn Long"),
                ("2", "Ambience"),
                ("3", "Ambience (Thick)"),
                ("4", "Ambience (Thick) - Loss"),
                ("5", "Cathedral"),
                ("6", "Cathedral - Loss"),
                ("7", "Drum Chamber"),
                ("8", "Garage"),
                ("9", "Garage (Bright)"),
                ("10", "Gymnasium"),
                ("11", "Gymnasium (Bright)"),
                ("12", "Gymnasium (Bright) - Loss"),
                ("13", "Hall (Small)"),
                ("14", "Hall (Medium)"),
                ("15", "Hall (Large)"),
                ("16", "Hall (Large) - Loss"),
                ("17", "Plate (Small)"),
                ("18", "Plate (Medium)"),
                ("19", "Plate (Large)"),
                ("20", "Plate (Large) - Loss"),
                ("21", "Pulse Chamber"),
                ("22", "Pulse Chamber (Reverse)"),
                ("23", "Resonator (96 ms)"),
                ("24", "Resonator (152 ms)"),
                ("25", "Resonator (208 ms)"),
                ("26", "Room (Small)"),
                ("27", "Room (Medium)"),
                ("28", "Room (Large)"),
                ("29", "Room (Large) - Loss"),
                ("30", "Slap Chamber"),
                ("31", "Slap Chamber - Loss"),
                ("32", "Slap Chamber (Bright)"),
                ("33", "Slap Chamber (Bright) - Loss"),
                ("34", "Smooth Hall (Small)"),
                ("35", "Smooth Hall (Medium)"),
                ("36", "Smooth Hall (Large)"),
                ("37", "Smooth Hall (Large) - Loss"),
                ("38", "Vocal Plate"),
                ("39", "Vocal Plate - Loss"),
                ("40", "Warble Chamber"),
                ("41", "Warehouse"),
                ("42", "Warehouse - Loss"),
            ]),
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
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)?;
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;
    let mode_str = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let mode: f32 = mode_str.parse().unwrap_or(0.0);

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let control_ports = &[
        (PORT_DECAY, decay),
        (PORT_DRY_LEVEL, dry_level),
        (PORT_WET_LEVEL, wet_level),
        (PORT_COMBS_EN, 1.0),
        (PORT_ALLPS_EN, 1.0),
        (PORT_BANDPASS_EN, 1.0),
        (PORT_STEREO_ENH, 1.0),
        (PORT_MODE, mode),
    ];

    match layout {
        AudioChannelLayout::Mono => {
            let processor = lv2::build_lv2_processor(
                &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
                &[PORT_AUDIO_IN_L], &[PORT_AUDIO_OUT_L], control_ports,
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
