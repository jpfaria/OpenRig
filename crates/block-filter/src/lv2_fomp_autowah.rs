use crate::registry::FilterModelDefinition;
use crate::FilterBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_fomp_autowah";
pub const DISPLAY_NAME: &str = "Auto-Wah";
const BRAND: &str = "fomp";
const PLUGIN_URI: &str = "http://drobilla.net/plugins/fomp/autowah";
const PLUGIN_DIR: &str = "fomp-autowah";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "autowah.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "autowah.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "autowah.dll";

const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_DRIVE: usize = 2;
const PORT_DECAY: usize = 3;
const PORT_RANGE: usize = 4;
const PORT_FREQ: usize = 5;
const PORT_MIX: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_FILTER.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("drive", "Drive", None, Some(0.0), -20.0, 20.0, 1.0, ParameterUnit::Decibels),
            float_parameter("decay", "Decay", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("range", "Range", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("freq", "Freq", None, Some(25.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", None, Some(75.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, drive: f32, decay: f32, range: f32, freq: f32, mix: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_DRIVE, drive), (PORT_DECAY, decay / 100.0), (PORT_RANGE, range / 100.0),
          (PORT_FREQ, freq / 100.0), (PORT_MIX, mix / 100.0)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let drive = required_f32(params, "drive").map_err(anyhow::Error::msg)?;
    let decay = required_f32(params, "decay").map_err(anyhow::Error::msg)?;
    let range = required_f32(params, "range").map_err(anyhow::Error::msg)?;
    let freq = required_f32(params, "freq").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, drive, decay, range, freq, mix)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, drive, decay, range, freq, mix)?;
            let right = build_mono(sample_rate, drive, decay, range, freq, mix)?;
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
