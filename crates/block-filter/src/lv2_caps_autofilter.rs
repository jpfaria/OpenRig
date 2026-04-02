use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{enum_parameter, float_parameter, required_f32, required_string, ModelParameterSchema, ParameterSet, ParameterUnit};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_caps_autofilter";
pub const DISPLAY_NAME: &str = "AutoFilter";
const BRAND: &str = "caps";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/AutoFilter";
const PLUGIN_DIR: &str = "mod-caps-AutoFilter.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "AutoFilter.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "AutoFilter.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "AutoFilter.dll";

// LV2 port indices (from AutoFilter.ttl)
// 0: Mode (0=LP, 1=HP, default 0)
// 1: Filter (0-1, default 1)
// 2: Frequency (20-3400, default 2500)
// 3: Q (0-1, default 0.27)
// 4: Depth (0-1, default 0.81)
// 5: LFO/Envelope (0-1, default 0.16)
// 6: Rate (0-1, default 0.26)
// 7: X/Z (0-1, default 0.35)
// 8: Audio In
// 9: Audio Out
const PORT_AUDIO_IN: usize = 8;
const PORT_AUDIO_OUT: usize = 9;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter("mode", "Mode", None, Some("lp"), &[("lp", "Low Pass"), ("hp", "High Pass")]),
            float_parameter("frequency", "Frequency", None, Some(2500.0), 20.0, 3400.0, 1.0, ParameterUnit::Hertz),
            float_parameter("q", "Q", None, Some(0.27), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("depth", "Depth", None, Some(0.81), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("lfoenv", "LFO/Env", None, Some(0.16), 0.0, 1.0, 0.01, ParameterUnit::None),
            float_parameter("rate", "Rate", None, Some(0.26), 0.0, 1.0, 0.01, ParameterUnit::None),
        ],
    })
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

struct DualMonoLv2 {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(
    sample_rate: f32,
    mode: f32,
    frequency: f32,
    q: f32,
    depth: f32,
    lfoenv: f32,
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
            (0, mode),
            (1, 1.0), // filter on
            (2, frequency),
            (3, q),
            (4, depth),
            (5, lfoenv),
            (6, rate),
            (7, 0.35), // x_z default
        ],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let mode_str = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let mode: f32 = if mode_str == "hp" { 1.0 } else { 0.0 };
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let q        = required_f32(params, "q").map_err(anyhow::Error::msg)?;
    let depth    = required_f32(params, "depth").map_err(anyhow::Error::msg)?;
    let lfoenv   = required_f32(params, "lfoenv").map_err(anyhow::Error::msg)?;
    let rate     = required_f32(params, "rate").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(build_mono_processor(sample_rate, mode, frequency, q, depth, lfoenv, rate)?)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, mode, frequency, q, depth, lfoenv, rate)?;
            let right = build_mono_processor(sample_rate, mode, frequency, q, depth, lfoenv, rate)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: FilterBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
