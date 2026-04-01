use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_larynx";
pub const DISPLAY_NAME: &str = "Larynx";
const BRAND: &str = "shiro";

const PLUGIN_URI: &str = "https://github.com/ninodewit/SHIRO-Plugins/plugins/larynx";
const PLUGIN_DIR: &str = "Larynx.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Larynx_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Larynx_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Larynx_dsp.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_TONE: usize = 2;
const PORT_DEPTH: usize = 3;
const PORT_RATE: usize = 4;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter(
                "tone",
                "Tone",
                None,
                Some(6000.0),
                500.0,
                12000.0,
                1.0,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "depth",
                "Depth",
                None,
                Some(1.0),
                0.1,
                5.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "rate",
                "Rate",
                None,
                Some(5.0),
                0.1,
                10.0,
                0.1,
                ParameterUnit::Hertz,
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
    tone: f32,
    depth: f32,
    rate: f32,
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
            (PORT_TONE, tone),
            (PORT_DEPTH, depth),
            (PORT_RATE, rate),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let tone = required_f32(params, "tone").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, tone, depth, rate)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, tone, depth, rate)?;
            let right = build_mono_processor(sample_rate, tone, depth, rate)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
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
