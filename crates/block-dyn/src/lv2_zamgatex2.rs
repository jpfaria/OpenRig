use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_zamgatex2";
pub const DISPLAY_NAME: &str = "ZamGate X2";
const BRAND: &str = "zam";
const PLUGIN_URI: &str = "urn:zamaudio:ZamGateX2";
const PLUGIN_DIR: &str = "ZamGateX2";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "ZamGateX2_dsp.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "ZamGateX2_dsp.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "ZamGateX2_dsp.dll";

const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_IN_R: usize = 1;
// port 2 = sidechain L (not connected)
// port 3 = sidechain R (not connected)
const PORT_AUDIO_OUT_L: usize = 4;
const PORT_AUDIO_OUT_R: usize = 5;
const PORT_ATTACK: usize = 6;
const PORT_RELEASE: usize = 7;
const PORT_THRESHOLD: usize = 8;
const PORT_MAKEUP: usize = 9;
const PORT_MAX_GATE_CLOSE: usize = 12;
const PORT_STEREO_LINK: usize = 13;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DYNAMICS.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("attack", "Attack", None, Some(50.0), 0.1, 500.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("release", "Release", None, Some(100.0), 0.1, 500.0, 0.1, ParameterUnit::Milliseconds),
            float_parameter("threshold", "Threshold", None, Some(-60.0), -80.0, 0.0, 1.0, ParameterUnit::Decibels),
            float_parameter("makeup", "Makeup", None, Some(0.0), -30.0, 30.0, 1.0, ParameterUnit::Decibels),
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
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    let control_ports = &[
        (PORT_ATTACK, attack), (PORT_RELEASE, release), (PORT_THRESHOLD, threshold),
        (PORT_MAKEUP, makeup), (PORT_MAX_GATE_CLOSE, -50.0), (PORT_STEREO_LINK, 1.0),
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
