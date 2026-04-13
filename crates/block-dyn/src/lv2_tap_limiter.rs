use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_limiter";
pub const DISPLAY_NAME: &str = "TAP Scaling Limiter";
const BRAND: &str = "tap";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/limiter";
const PLUGIN_DIR: &str = "tap-limiter";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_limiter.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_limiter.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_limiter.dll";

// Ports: 0=limit_level, 1=out_volume, 2=latency(output), 3=AudioIn, 4=AudioOut
const PORT_LIMIT_LEVEL: usize = 0;
const PORT_OUT_VOLUME: usize = 1;
const PORT_AUDIO_IN: usize = 3;
const PORT_AUDIO_OUT: usize = 4;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("limit_level", "Limit Level", None, Some(0.0), -30.0, 20.0, 1.0, ParameterUnit::Decibels),
            float_parameter("out_volume", "Output Volume", None, Some(0.0), -30.0, 20.0, 1.0, ParameterUnit::Decibels),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, limit_level: f32, out_volume: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_LIMIT_LEVEL, limit_level), (PORT_OUT_VOLUME, out_volume)],
        &[2], // latency meter output
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let limit_level = required_f32(params, "limit_level").map_err(anyhow::Error::msg)?;
    let out_volume = required_f32(params, "out_volume").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, limit_level, out_volume)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, limit_level, out_volume)?;
            let right = build_mono(sample_rate, limit_level, out_volume)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
