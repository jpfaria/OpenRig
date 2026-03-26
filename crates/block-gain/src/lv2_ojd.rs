use crate::registry::GainModelDefinition;
use crate::GainBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema,
    ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_ojd";
pub const DISPLAY_NAME: &str = "OJD";
const BRAND: &str = "schrammel";

const PLUGIN_URI: &str = "https://github.com/JanosGit/Schrammel_OJD";
const PLUGIN_DIR: &str = "OJD.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "OJD.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "OJD.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "OJD.dll";

// LV2 port indices (from TTL)
const PORT_FREEWHEEL: usize = 0;
const PORT_AUDIO_IN: usize = 1;
const PORT_AUDIO_OUT: usize = 2;
const PORT_DRIVE: usize = 3;
const PORT_TONE: usize = 4;
const PORT_VOLUME: usize = 5;
const PORT_HP_LP: usize = 6;
const PORT_ENABLED: usize = 7;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_GAIN.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "drive",
                "Drive",
                None,
                Some(16.5),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "tone",
                "Tone",
                None,
                Some(50.0),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            float_parameter(
                "volume",
                "Volume",
                None,
                Some(83.5),
                0.0,
                100.0,
                1.0,
                ParameterUnit::Percent,
            ),
            enum_parameter(
                "hp_lp",
                "HP / LP",
                None,
                Some("lp"),
                &[("lp", "LP"), ("hp", "HP")],
            ),
        ],
    }
}

fn validate_params(params: &ParameterSet) -> Result<()> {
    let _ = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let _ = required_f32(params, "volume").map_err(anyhow::Error::msg)?;
    let _ = required_string(params, "hp_lp").map_err(anyhow::Error::msg)?;
    Ok(())
}

fn asset_summary(_params: &ParameterSet) -> Result<String> {
    Ok(format!("lv2='{}'", MODEL_ID))
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
    drive: f32,
    tone: f32,
    volume: f32,
    hp_lp: f32,
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
            (PORT_DRIVE, drive),
            (PORT_TONE, tone),
            (PORT_VOLUME, volume),
            (PORT_HP_LP, hp_lp),
            (PORT_ENABLED, 1.0),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)? / 100.0;
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)? / 100.0;
    let volume = required_f32(params, "volume").map_err(anyhow::Error::msg)? / 100.0;
    let hp_lp_str = required_string(params, "hp_lp").map_err(anyhow::Error::msg)?;
    let hp_lp: f32 = if hp_lp_str == "hp" { 1.0 } else { 0.0 };

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, drive, tone, volume, hp_lp)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, drive, tone, volume, hp_lp)?;
            let right = build_mono_processor(sample_rate, drive, tone, volume, hp_lp)?;
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
