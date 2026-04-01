// @platform: macos
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_tremolo";
pub const DISPLAY_NAME: &str = "TAP Tremolo";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/tremolo";
const PLUGIN_DIR: &str = "tap-tremolo.lv2";
const PLUGIN_BINARY: &str = "tap_tremolo.dylib";

// LV2 port indices (from tap_tremolo.ttl)
// Controls: 0=frequency, 1=depth, 2=gain
// Audio: 3=input_0, 4=output_0
const PORT_FREQUENCY: usize = 0;
const PORT_DEPTH: usize = 1;
const PORT_GAIN: usize = 2;
const PORT_AUDIO_IN: usize = 3;
const PORT_AUDIO_OUT: usize = 4;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("rate_hz", "Rate", None, Some(5.0), 0.0, 20.0, 0.1, ParameterUnit::Hertz),
            float_parameter("depth", "Depth", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
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

fn build_mono(sample_rate: f32, rate_hz: f32, depth: f32) -> Result<lv2::Lv2Processor> {
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
            (PORT_FREQUENCY, rate_hz),
            (PORT_DEPTH, depth),
            (PORT_GAIN, 0.0),
        ],
    )
}

struct DualMonoTremolo {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoTremolo {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let rate_hz = required_f32(params, "rate_hz").map_err(anyhow::Error::msg)?;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, rate_hz, depth)?)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, rate_hz, depth)?;
            let right = build_mono(sample_rate, rate_hz, depth)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoTremolo { left, right })))
        }
    }
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
