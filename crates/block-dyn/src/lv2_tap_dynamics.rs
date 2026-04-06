use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    enum_parameter, float_parameter, required_f32, required_string,
    ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_tap_dynamics";
pub const DISPLAY_NAME: &str = "TAP Dynamics";
const BRAND: &str = "tap";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/dynamics";
const PLUGIN_DIR: &str = "tap-dynamics";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "tap_dynamics.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "tap_dynamics.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "tap_dynamics.dll";

// Ports: 0=attack, 1=release, 2=offset_gain, 3=makeup_gain, 4=envelope, 5=adj, 6=function, 7=AudioIn, 8=AudioOut
const PORT_ATTACK: usize = 0;
const PORT_RELEASE: usize = 1;
const PORT_OFFSET_GAIN: usize = 2;
const PORT_MAKEUP_GAIN: usize = 3;
const PORT_FUNCTION: usize = 6;
const PORT_AUDIO_IN: usize = 7;
const PORT_AUDIO_OUT: usize = 8;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("attack", "Attack", None, Some(128.0), 4.0, 500.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("release", "Release", None, Some(502.0), 4.0, 1000.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("offset_gain", "Offset Gain", None, Some(0.0), -20.0, 20.0, 1.0, ParameterUnit::Decibels),
            float_parameter("makeup_gain", "Makeup Gain", None, Some(0.0), -20.0, 20.0, 1.0, ParameterUnit::Decibels),
            enum_parameter("function", "Function", None, Some("0"), &[
                ("0", "Compressor (2:1)"),
                ("1", "Compressor (3:1)"),
                ("2", "Compressor (5:1)"),
                ("3", "Limiter (10:1)"),
                ("4", "Gate (-30dB)"),
                ("5", "Gate (-60dB)"),
                ("6", "Gate (-inf)"),
            ]),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, attack: f32, release: f32, offset: f32, makeup: f32, function: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_ATTACK, attack), (PORT_RELEASE, release), (PORT_OFFSET_GAIN, offset),
          (PORT_MAKEUP_GAIN, makeup), (4, 0.0), (5, 0.0), (PORT_FUNCTION, function)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let offset = required_f32(params, "offset_gain").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup_gain").map_err(anyhow::Error::msg)?;
    let func_str = required_string(params, "function").map_err(anyhow::Error::msg)?;
    let function: f32 = func_str.parse().unwrap_or(0.0);
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, attack, release, offset, makeup, function)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, attack, release, offset, makeup, function)?;
            let right = build_mono(sample_rate, attack, release, offset, makeup, function)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
