use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_invada_hpf";
pub const DISPLAY_NAME: &str = "Invada High Pass";
const BRAND: &str = "invada";
const PLUGIN_URI: &str = "http://invadarecords.com/plugins/lv2/filter/hpf/mono";
const PLUGIN_DIR: &str = "invada-filter";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "inv_filter.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "inv_filter.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "inv_filter.dll";

const PORT_BYPASS: usize = 0;
const PORT_FREQUENCY: usize = 1;
const PORT_GAIN: usize = 2;
const PORT_SOFT_CLIP: usize = 3;
const PORT_AUDIO_IN: usize = 6;
const PORT_AUDIO_OUT: usize = 7;
const PORT_DRIVE: usize = 8;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("frequency", "Frequency", None, Some(200.0), 20.0, 20000.0, 10.0, ParameterUnit::Hertz),
            float_parameter("gain", "Gain", None, Some(0.0), 0.0, 12.0, 1.0, ParameterUnit::Decibels),
            float_parameter("drive", "Drive", None, Some(0.0), 0.0, 10.0, 1.0, ParameterUnit::None),
        ],
    }
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, frequency: f32, gain: f32, drive: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_BYPASS, 0.0), (PORT_FREQUENCY, frequency), (PORT_GAIN, gain),
          (PORT_SOFT_CLIP, 0.0), (PORT_DRIVE, drive)],
        &[],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let frequency = required_f32(params, "frequency").map_err(anyhow::Error::msg)?;
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, frequency, gain, drive)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, frequency, gain, drive)?;
            let right = build_mono(sample_rate, frequency, gain, drive)?;
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
