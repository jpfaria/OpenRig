use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tap_chorus_flanger";
pub const DISPLAY_NAME: &str = "TAP Chorus/Flanger";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/chorusflanger";
const PLUGIN_DIR: &str = "tap-chorusflanger.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_chorusflanger.dll";

// LV2 port indices (from TTL)
const PORT_FREQUENCY: usize = 0;
const PORT_LR_PHASE_SHIFT: usize = 1;
const PORT_DEPTH: usize = 2;
const PORT_DELAY: usize = 3;
const PORT_CONTOUR: usize = 4;
const PORT_DRY_LEVEL: usize = 5;
const PORT_WET_LEVEL: usize = 6;
const PORT_AUDIO_IN_L: usize = 7;
const PORT_AUDIO_OUT_L: usize = 9;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "frequency",
                "Frequency",
                None,
                Some(1.75),
                0.0,
                5.0,
                0.01,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "lr_phase_shift",
                "L/R Phase Shift",
                None,
                Some(90.0),
                0.0,
                180.0,
                1.0,
                ParameterUnit::None,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(75.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "delay",
                "Delay",
                None,
                Some(25.0),
                0.0,
                100.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "contour",
                "Contour",
                None,
                Some(100.0),
                20.0,
                20000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "dry_level",
                "Dry Level",
                None,
                Some(-3.0),
                -90.0,
                20.0,
                0.1,
                ParameterUnit::Decibels,
            ),
            float_parameter(
                "wet_level",
                "Wet Level",
                None,
                Some(-3.0),
                -90.0,
                20.0,
                0.1,
                ParameterUnit::Decibels,
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

fn build_mono_processor(
    sample_rate: f32,
    frequency: f32,
    lr_phase_shift: f32,
    depth: f32,
    delay: f32,
    contour: f32,
    dry_level: f32,
    wet_level: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN_L],
        &[PORT_AUDIO_OUT_L],
        &[
            (PORT_FREQUENCY, frequency),
            (PORT_LR_PHASE_SHIFT, lr_phase_shift),
            (PORT_DEPTH, depth),
            (PORT_DELAY, delay),
            (PORT_CONTOUR, contour),
            (PORT_DRY_LEVEL, dry_level),
            (PORT_WET_LEVEL, wet_level),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let lr_phase_shift = required_f32(params, "lr_phase_shift").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let delay = required_f32(params, "delay").map_err(anyhow::Error::msg)?;
    let contour = required_f32(params, "contour").map_err(anyhow::Error::msg)?;
    let dry_level = required_f32(params, "dry_level").map_err(anyhow::Error::msg)?;
    let wet_level = required_f32(params, "wet_level").map_err(anyhow::Error::msg)?;

    let _ = layout; // DualMono: engine always calls builder with Mono
    let processor = build_mono_processor(
        sample_rate,
        frequency,
        lr_phase_shift,
        depth,
        delay,
        contour,
        dry_level,
        wet_level,
    )?;
    Ok(BlockProcessor::Mono(Box::new(processor)))
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
