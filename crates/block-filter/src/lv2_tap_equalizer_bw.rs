use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_equalizer_bw";
pub const DISPLAY_NAME: &str = "TAP Equalizer/BW";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/eqbw";
const PLUGIN_DIR: &str = "tap-eqbw.lv2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_eqbw.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_eqbw.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_eqbw.dll";

// LV2 port indices (from TTL)
const PORT_BAND1_GAIN: usize = 0;
const PORT_BAND2_GAIN: usize = 1;
const PORT_BAND3_GAIN: usize = 2;
const PORT_BAND4_GAIN: usize = 3;
const PORT_BAND5_GAIN: usize = 4;
const PORT_BAND6_GAIN: usize = 5;
const PORT_BAND7_GAIN: usize = 6;
const PORT_BAND8_GAIN: usize = 7;
const PORT_BAND1_FREQ: usize = 8;
const PORT_BAND2_FREQ: usize = 9;
const PORT_BAND3_FREQ: usize = 10;
const PORT_BAND4_FREQ: usize = 11;
const PORT_BAND5_FREQ: usize = 12;
const PORT_BAND6_FREQ: usize = 13;
const PORT_BAND7_FREQ: usize = 14;
const PORT_BAND8_FREQ: usize = 15;
const PORT_BAND1_BW: usize = 16;
const PORT_BAND2_BW: usize = 17;
const PORT_BAND3_BW: usize = 18;
const PORT_BAND4_BW: usize = 19;
const PORT_BAND5_BW: usize = 20;
const PORT_BAND6_BW: usize = 21;
const PORT_BAND7_BW: usize = 22;
const PORT_BAND8_BW: usize = 23;
const PORT_AUDIO_IN: usize = 24;
const PORT_AUDIO_OUT: usize = 25;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("band1_gain", "Band 1 Gain", Some("Band 1"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band1_freq", "Band 1 Freq", Some("Band 1"), Some(100.0), 40.0, 280.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band1_bw", "Band 1 BW", Some("Band 1"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band2_gain", "Band 2 Gain", Some("Band 2"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band2_freq", "Band 2 Freq", Some("Band 2"), Some(200.0), 100.0, 500.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band2_bw", "Band 2 BW", Some("Band 2"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band3_gain", "Band 3 Gain", Some("Band 3"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band3_freq", "Band 3 Freq", Some("Band 3"), Some(400.0), 200.0, 1000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band3_bw", "Band 3 BW", Some("Band 3"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band4_gain", "Band 4 Gain", Some("Band 4"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band4_freq", "Band 4 Freq", Some("Band 4"), Some(1000.0), 400.0, 2800.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band4_bw", "Band 4 BW", Some("Band 4"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band5_gain", "Band 5 Gain", Some("Band 5"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band5_freq", "Band 5 Freq", Some("Band 5"), Some(3000.0), 1000.0, 5000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band5_bw", "Band 5 BW", Some("Band 5"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band6_gain", "Band 6 Gain", Some("Band 6"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band6_freq", "Band 6 Freq", Some("Band 6"), Some(6000.0), 3000.0, 9000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band6_bw", "Band 6 BW", Some("Band 6"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band7_gain", "Band 7 Gain", Some("Band 7"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band7_freq", "Band 7 Freq", Some("Band 7"), Some(12000.0), 6000.0, 18000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band7_bw", "Band 7 BW", Some("Band 7"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
            float_parameter("band8_gain", "Band 8 Gain", Some("Band 8"), Some(0.0), -50.0, 20.0, 0.1, ParameterUnit::Decibels),
            float_parameter("band8_freq", "Band 8 Freq", Some("Band 8"), Some(15000.0), 10000.0, 20000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("band8_bw", "Band 8 BW", Some("Band 8"), Some(1.0), 0.1, 5.0, 0.1, ParameterUnit::None),
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

#[allow(clippy::too_many_arguments)]
fn build_mono_processor(
    sample_rate: f32,
    band1_gain: f32, band1_freq: f32, band1_bw: f32,
    band2_gain: f32, band2_freq: f32, band2_bw: f32,
    band3_gain: f32, band3_freq: f32, band3_bw: f32,
    band4_gain: f32, band4_freq: f32, band4_bw: f32,
    band5_gain: f32, band5_freq: f32, band5_bw: f32,
    band6_gain: f32, band6_freq: f32, band6_bw: f32,
    band7_gain: f32, band7_freq: f32, band7_bw: f32,
    band8_gain: f32, band8_freq: f32, band8_bw: f32,
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
            (PORT_BAND1_GAIN, band1_gain),
            (PORT_BAND2_GAIN, band2_gain),
            (PORT_BAND3_GAIN, band3_gain),
            (PORT_BAND4_GAIN, band4_gain),
            (PORT_BAND5_GAIN, band5_gain),
            (PORT_BAND6_GAIN, band6_gain),
            (PORT_BAND7_GAIN, band7_gain),
            (PORT_BAND8_GAIN, band8_gain),
            (PORT_BAND1_FREQ, band1_freq),
            (PORT_BAND2_FREQ, band2_freq),
            (PORT_BAND3_FREQ, band3_freq),
            (PORT_BAND4_FREQ, band4_freq),
            (PORT_BAND5_FREQ, band5_freq),
            (PORT_BAND6_FREQ, band6_freq),
            (PORT_BAND7_FREQ, band7_freq),
            (PORT_BAND8_FREQ, band8_freq),
            (PORT_BAND1_BW, band1_bw),
            (PORT_BAND2_BW, band2_bw),
            (PORT_BAND3_BW, band3_bw),
            (PORT_BAND4_BW, band4_bw),
            (PORT_BAND5_BW, band5_bw),
            (PORT_BAND6_BW, band6_bw),
            (PORT_BAND7_BW, band7_bw),
            (PORT_BAND8_BW, band8_bw),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let band1_gain = required_f32(params, "band1_gain").map_err(anyhow::Error::msg)?;
    let band1_freq = required_f32(params, "band1_freq").map_err(anyhow::Error::msg)?;
    let band1_bw = required_f32(params, "band1_bw").map_err(anyhow::Error::msg)?;
    let band2_gain = required_f32(params, "band2_gain").map_err(anyhow::Error::msg)?;
    let band2_freq = required_f32(params, "band2_freq").map_err(anyhow::Error::msg)?;
    let band2_bw = required_f32(params, "band2_bw").map_err(anyhow::Error::msg)?;
    let band3_gain = required_f32(params, "band3_gain").map_err(anyhow::Error::msg)?;
    let band3_freq = required_f32(params, "band3_freq").map_err(anyhow::Error::msg)?;
    let band3_bw = required_f32(params, "band3_bw").map_err(anyhow::Error::msg)?;
    let band4_gain = required_f32(params, "band4_gain").map_err(anyhow::Error::msg)?;
    let band4_freq = required_f32(params, "band4_freq").map_err(anyhow::Error::msg)?;
    let band4_bw = required_f32(params, "band4_bw").map_err(anyhow::Error::msg)?;
    let band5_gain = required_f32(params, "band5_gain").map_err(anyhow::Error::msg)?;
    let band5_freq = required_f32(params, "band5_freq").map_err(anyhow::Error::msg)?;
    let band5_bw = required_f32(params, "band5_bw").map_err(anyhow::Error::msg)?;
    let band6_gain = required_f32(params, "band6_gain").map_err(anyhow::Error::msg)?;
    let band6_freq = required_f32(params, "band6_freq").map_err(anyhow::Error::msg)?;
    let band6_bw = required_f32(params, "band6_bw").map_err(anyhow::Error::msg)?;
    let band7_gain = required_f32(params, "band7_gain").map_err(anyhow::Error::msg)?;
    let band7_freq = required_f32(params, "band7_freq").map_err(anyhow::Error::msg)?;
    let band7_bw = required_f32(params, "band7_bw").map_err(anyhow::Error::msg)?;
    let band8_gain = required_f32(params, "band8_gain").map_err(anyhow::Error::msg)?;
    let band8_freq = required_f32(params, "band8_freq").map_err(anyhow::Error::msg)?;
    let band8_bw = required_f32(params, "band8_bw").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(
                sample_rate,
                band1_gain, band1_freq, band1_bw,
                band2_gain, band2_freq, band2_bw,
                band3_gain, band3_freq, band3_bw,
                band4_gain, band4_freq, band4_bw,
                band5_gain, band5_freq, band5_bw,
                band6_gain, band6_freq, band6_bw,
                band7_gain, band7_freq, band7_bw,
                band8_gain, band8_freq, band8_bw,
            )?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(
                sample_rate,
                band1_gain, band1_freq, band1_bw,
                band2_gain, band2_freq, band2_bw,
                band3_gain, band3_freq, band3_bw,
                band4_gain, band4_freq, band4_bw,
                band5_gain, band5_freq, band5_bw,
                band6_gain, band6_freq, band6_bw,
                band7_gain, band7_freq, band7_bw,
                band8_gain, band8_freq, band8_bw,
            )?;
            let right = build_mono_processor(
                sample_rate,
                band1_gain, band1_freq, band1_bw,
                band2_gain, band2_freq, band2_bw,
                band3_gain, band3_freq, band3_bw,
                band4_gain, band4_freq, band4_bw,
                band5_gain, band5_freq, band5_bw,
                band6_gain, band6_freq, band6_bw,
                band7_gain, band7_freq, band7_bw,
                band8_gain, band8_freq, band8_bw,
            )?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
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
