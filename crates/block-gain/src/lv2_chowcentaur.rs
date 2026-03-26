use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_chowcentaur";
pub const DISPLAY_NAME: &str = "Centaur";
const BRAND: &str = "chowdsp";

const PLUGIN_URI: &str = "https://github.com/jatinchowdhury18/KlonCentaur";
const PLUGIN_DIR: &str = "ChowCentaur.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ChowCentaur.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ChowCentaur.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ChowCentaur.dll";

// LV2 port indices (from TTL)
const PORT_FREEWHEEL: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_AUDIO_OUT: usize = 2;
const PORT_GAIN: usize = 3;
const PORT_TREBLE: usize = 4;
const PORT_LEVEL: usize = 5;
const PORT_MODE: usize = 6;
const PORT_ENABLED: usize = 7;
const PORT_MONO: usize = 8;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "gain",
                "Gain",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "treble",
                "Treble",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "level",
                "Level",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            enum_parameter(
                "mode",
                "Mode",
                None,
                Some("traditional"),
                &[
                    ("traditional", "Traditional"),
                    ("neural", "Neural"),
                ],
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "treble").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "level").map_err(anyhow::Error::msg)?;
    let _ = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("lv2='{}'", MODEL_ID))
}

fn resolve_lib_path() -> Result<String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    // Try relative to executable first, then relative to CWD
    let candidates = [
        exe_dir.as_ref().map(|d| d.join("../../").join(lv2::default_lv2_lib_dir()).join(PLUGIN_BINARY)),
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

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [
            self.left.process_sample(input[0]),
            self.right.process_sample(input[1]),
        ]
    }
}

fn build_mono_processor(
    sample_rate: f32,
    gain: f32,
    treble: f32,
    level: f32,
    mode: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = resolve_lib_path()?;
    let bundle_path = resolve_bundle_path()?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_FREEWHEEL, 0.0),
            (PORT_GAIN, gain),
            (PORT_TREBLE, treble),
            (PORT_LEVEL, level),
            (PORT_MODE, mode),
            (PORT_ENABLED, 1.0),
            (PORT_MONO, 1.0),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)? / 100.0;
    let treble = required_f32(params, "treble").map_err(anyhow::Error::msg)? / 100.0;
    let level = required_f32(params, "level").map_err(anyhow::Error::msg)? / 100.0;
    let mode_str = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let mode: f32 = if mode_str == "neural" { 1.0 } else { 0.0 };

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, gain, treble, level, mode)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, gain, treble, level, mode)?;
            let right = build_mono_processor(sample_rate, gain, treble, level, mode)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
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
