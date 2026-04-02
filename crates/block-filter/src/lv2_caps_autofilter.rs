use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_caps_autofilter";
pub const DISPLAY_NAME: &str = "CAPS AutoFilter";
const BRAND: &str = "caps";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/AutoFilter";
const PLUGIN_DIR: &str = "mod-caps-AutoFilter";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "AutoFilter.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "AutoFilter.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "AutoFilter.dll";

const PORT_MODE: usize = 0;
const PORT_FILTER: usize = 1;
const PORT_FREQUENCY: usize = 2;
const PORT_Q: usize = 3;
const PORT_DEPTH: usize = 4;
const PORT_LFO_ENV: usize = 5;
const PORT_RATE: usize = 6;
const PORT_XZ: usize = 7;
const PORT_AUDIO_IN: usize = 8;
const PORT_AUDIO_OUT: usize = 9;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            enum_parameter("mode", "Mode", None, Some("lowpass"), &[
                ("lowpass", "Low Pass"),
                ("bandpass", "Band Pass"),
            ]),
            float_parameter("frequency", "Frequency", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("q", "Q", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("depth", "Depth", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("rate", "Rate", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, mode: f32, freq: f32, q: f32, depth: f32, rate: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_MODE, mode), (PORT_FILTER, 0.0), (PORT_FREQUENCY, freq),
          (PORT_Q, q), (PORT_DEPTH, depth), (PORT_LFO_ENV, 0.5),
          (PORT_RATE, rate), (PORT_XZ, 0.5)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let mode_str = required_string(params, "mode").map_err(anyhow::Error::msg)?;
    let mode: f32 = if mode_str == "bandpass" { 1.0 } else { 0.0 };
    let freq = required_f32(params, "frequency").map_err(anyhow::Error::msg)? / 100.0;
    let q = required_f32(params, "q").map_err(anyhow::Error::msg)? / 100.0;
    let depth = required_f32(params, "depth").map_err(anyhow::Error::msg)? / 100.0;
    let rate = required_f32(params, "rate").map_err(anyhow::Error::msg)? / 100.0;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, mode, freq, q, depth, rate)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, mode, freq, q, depth, rate)?;
            let right = build_mono(sample_rate, mode, freq, q, depth, rate)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: FilterModelDefinition = FilterModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: FilterBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
