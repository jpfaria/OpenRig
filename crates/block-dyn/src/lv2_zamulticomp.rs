use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_zamulticomp";
pub const DISPLAY_NAME: &str = "ZaMultiComp";
const BRAND: &str = "zam";
const PLUGIN_URI: &str = "urn:zamaudio:ZaMultiComp";
const PLUGIN_DIR: &str = "ZaMultiComp";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ZaMultiComp_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ZaMultiComp_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ZaMultiComp_dsp.dll";

// Mono: 0=AudioIn, 1=AudioOut, 2+=control (3 bands)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_ATTACK1: usize = 2;
const PORT_RELEASE1: usize = 3;
const PORT_KNEE1: usize = 4;
const PORT_RATIO1: usize = 5;
const PORT_THRESHOLD1: usize = 6;
const PORT_MAKEUP1: usize = 7;
const PORT_XOVER1: usize = 20;
const PORT_XOVER2: usize = 21;
const PORT_MASTER: usize = 22;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("threshold", "Threshold", None, Some(0.0), -60.0, 0.0, 1.0, ParameterUnit::Decibels),
            float_parameter("ratio", "Ratio", None, Some(4.0), 1.0, 20.0, 1.0, ParameterUnit::Ratio),
            float_parameter("attack", "Attack", None, Some(10.0), 0.1, 100.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("release", "Release", None, Some(80.0), 1.0, 500.0, 1.0, ParameterUnit::Milliseconds),
            float_parameter("makeup", "Makeup", None, Some(0.0), 0.0, 30.0, 1.0, ParameterUnit::Decibels),
            float_parameter("master", "Master", None, Some(0.0), -12.0, 12.0, 1.0, ParameterUnit::Decibels),
        ],
    })
}

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, threshold: f32, ratio: f32, attack: f32, release: f32, makeup: f32, master: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    // Apply same settings to all 3 bands for simplicity
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN], &[PORT_AUDIO_OUT],
        &[
            (PORT_ATTACK1, attack), (PORT_RELEASE1, release), (PORT_KNEE1, 0.0),
            (PORT_RATIO1, ratio), (PORT_THRESHOLD1, threshold), (PORT_MAKEUP1, makeup),
            (8, attack), (9, release), (10, 0.0), (11, ratio), (12, threshold), (13, makeup),
            (14, attack), (15, release), (16, 0.0), (17, ratio), (18, threshold), (19, makeup),
            (PORT_XOVER1, 250.0), (PORT_XOVER2, 1400.0), (PORT_MASTER, master),
        ],
        &[23, 24, 25, 26, 27, 28], // gain reduction + output level meters
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let ratio = required_f32(params, "ratio").map_err(anyhow::Error::msg)?;
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup").map_err(anyhow::Error::msg)?;
    let master = required_f32(params, "master").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, threshold, ratio, attack, release, makeup, master)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, threshold, ratio, attack, release, makeup, master)?;
            let right = build_mono(sample_rate, threshold, ratio, attack, release, makeup, master)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
