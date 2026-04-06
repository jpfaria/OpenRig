use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_caps_compress";
pub const DISPLAY_NAME: &str = "CAPS Compress";
const BRAND: &str = "caps";
const PLUGIN_URI: &str = "http://moddevices.com/plugins/caps/Compress";
const PLUGIN_DIR: &str = "mod-caps-Compress";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "Compress.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "Compress.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "Compress.dll";

// Ports: 0=measure, 1=mode, 2=threshold, 3=strength, 4=attack, 5=release, 6=gain, 7=state(out), 8=AudioIn, 9=AudioOut
const PORT_MEASURE: usize = 0;
const PORT_MODE: usize = 1;
const PORT_THRESHOLD: usize = 2;
const PORT_STRENGTH: usize = 3;
const PORT_ATTACK: usize = 4;
const PORT_RELEASE: usize = 5;
const PORT_GAIN: usize = 6;
const PORT_AUDIO_IN: usize = 8;
const PORT_AUDIO_OUT: usize = 9;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("threshold", "Threshold", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("strength", "Strength", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("attack", "Attack", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("release", "Release", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("gain", "Gain", None, Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, threshold: f32, strength: f32, attack: f32, release: f32, gain: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    lv2::build_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[(PORT_MEASURE, 0.0), (PORT_MODE, 0.0), (PORT_THRESHOLD, threshold / 100.0),
          (PORT_STRENGTH, strength / 100.0), (PORT_ATTACK, attack / 100.0),
          (PORT_RELEASE, release / 100.0), (PORT_GAIN, gain / 100.0)],
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let strength = required_f32(params, "strength").map_err(anyhow::Error::msg)?;
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let gain = required_f32(params, "gain").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, threshold, strength, attack, release, gain)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, threshold, strength, attack, release, gain)?;
            let right = build_mono(sample_rate, threshold, strength, attack, release, gain)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
