use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_zamulticompx2";
pub const DISPLAY_NAME: &str = "ZaMultiComp X2";
const BRAND: &str = "zam";
const PLUGIN_URI: &str = "urn:zamaudio:ZaMultiCompX2";
const PLUGIN_DIR: &str = "ZaMultiCompX2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ZaMultiCompX2_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ZaMultiCompX2_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ZaMultiCompX2_dsp.dll";

// Stereo: 0=InL, 1=InR, 2=OutL, 3=OutR, 4+=control
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
const PORT_AUDIO_OUT_L: usize = 2;
const PORT_AUDIO_OUT_R: usize = 3;
const PORT_ATTACK1: usize = 4;
const PORT_RELEASE1: usize = 5;
const PORT_KNEE1: usize = 6;
const PORT_RATIO1: usize = 7;
const PORT_THRESHOLD1: usize = 8;
const PORT_MAKEUP1: usize = 9;
const PORT_XOVER1: usize = 22;
const PORT_XOVER2: usize = 23;
const PORT_MASTER: usize = 24;
const PORT_STEREO_LINK: usize = 25;

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

struct StereoAsMono(lv2::StereoLv2Processor);
impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        (l + r) * 0.5
    }
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let ratio = required_f32(params, "ratio").map_err(anyhow::Error::msg)?;
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup").map_err(anyhow::Error::msg)?;
    let master = required_f32(params, "master").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    let control_ports = &[
        (PORT_ATTACK1, attack), (PORT_RELEASE1, release), (PORT_KNEE1, 0.0),
        (PORT_RATIO1, ratio), (PORT_THRESHOLD1, threshold), (PORT_MAKEUP1, makeup),
        (10, attack), (11, release), (12, 0.0), (13, ratio), (14, threshold), (15, makeup),
        (16, attack), (17, release), (18, 0.0), (19, ratio), (20, threshold), (21, makeup),
        (PORT_XOVER1, 250.0), (PORT_XOVER2, 1400.0), (PORT_MASTER, master),
        (PORT_STEREO_LINK, 1.0),
    ];

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R], &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        control_ports,
    )?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(StereoAsMono(processor)))),
        AudioChannelLayout::Stereo => Ok(BlockProcessor::Stereo(Box::new(processor))),
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
