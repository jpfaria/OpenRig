use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_avocado";
pub const DISPLAY_NAME: &str = "Avocado";
const BRAND: &str = "remaincalm";

const PLUGIN_URI: &str = "http://remaincalm.org/plugins/avocado";
const PLUGIN_DIR: &str = "avocado";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "avocado_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "avocado_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "avocado_dsp.dll";

// LV2 port indices (from TTL) — mono in, mono out
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_TIME: usize = 2;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("time", "Time", None, Some(70.0), 10.0, 250.0, 1.0, ParameterUnit::Milliseconds),
        ],
    }
}

struct DualMonoAvocado {
    left: lv2::Lv2Processor,
    right: lv2::Lv2Processor,
}

impl StereoProcessor for DualMonoAvocado {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono_processor(sample_rate: f32, time: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_TIME, time)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let time = required_f32(params, "time").map_err(anyhow::Error::msg)?;

    match layout {
        AudioChannelLayout::Mono => {
            let processor = build_mono_processor(sample_rate, time)?;
            Ok(BlockProcessor::Mono(Box::new(processor)))
        }
        AudioChannelLayout::Stereo => {
            let left = build_mono_processor(sample_rate, time)?;
            let right = build_mono_processor(sample_rate, time)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoAvocado { left, right })))
        }
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DelayBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
