use crate::registry::PitchModelDefinition;
use crate::PitchBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_repsycho";
pub const DISPLAY_NAME: &str = "MDA RePsycho!";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mda/RePsycho";
const PLUGIN_DIR: &str = "mod-mda-RePsycho.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "RePsycho.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "RePsycho.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "RePsycho.dll";

// LV2 port indices (from RePsycho.ttl)
const PORT_TUNE: usize = 0;
const PORT_FINE: usize = 1;
const PORT_DECAY: usize = 2;
const PORT_THRESH: usize = 3;
const PORT_HOLD: usize = 4;
const PORT_MIX: usize = 5;
const PORT_QUALITY: usize = 6;
const PORT_LEFT_IN: usize = 7;
const PORT_RIGHT_IN: usize = 8;
const PORT_LEFT_OUT: usize = 9;
const PORT_RIGHT_OUT: usize = 10;

fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: "pitch".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::TrueStereo,
        parameters: vec![
            float_parameter(
                "tune",
                "Tune",
                Some("Pitch"),
                Some(0.0),
                -24.0,
                0.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "fine",
                "Fine",
                Some("Pitch"),
                Some(0.0),
                -100.0,
                0.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "thresh",
                "Threshold",
                Some("Detection"),
                Some(-12.0),
                -30.0,
                0.0,
                0.5,
                ParameterUnit::None,
            ),
            float_parameter(
                "hold",
                "Hold",
                Some("Detection"),
                Some(122.5),
                10.0,
                260.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "mix",
                "Mix",
                Some("Control"),
                Some(100.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            enum_parameter(
                "quality",
                "Quality",
                Some("Control"),
                Some("high"),
                &[("low", "Low"), ("high", "High")],
            ),
        ],
    }
}

fn quality_to_float(s: &str) -> f32 {
    match s {
        "low" => 0.0,
        "high" => 1.0,
        _ => 1.0,
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
    let tune = required_f32(params, "tune").map_err(anyhow::Error::msg)?;
    let fine = required_f32(params, "fine").map_err(anyhow::Error::msg)?;
    let thresh = required_f32(params, "thresh").map_err(anyhow::Error::msg)?;
    let hold = required_f32(params, "hold").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let quality_str = required_string(params, "quality").map_err(anyhow::Error::msg)?;
    let quality = quality_to_float(&quality_str);

    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    let control_ports = &[
        (PORT_TUNE, tune),
        (PORT_FINE, fine),
        (PORT_DECAY, 0.0),
        (PORT_THRESH, thresh),
        (PORT_HOLD, hold),
        (PORT_MIX, mix),
        (PORT_QUALITY, quality),
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
