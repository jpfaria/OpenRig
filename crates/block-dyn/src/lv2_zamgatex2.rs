use crate::registry::DynModelDefinition;
use crate::DynBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

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

// Ports: 0=InL, 1=InR, 2=SidechainL, 3=SidechainR, 4=OutL, 5=OutR, 6+=control
const PORT_AUDIO_IN_L: usize = 0;
const PORT_AUDIO_OUT_L: usize = 4;
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

struct DualMonoLv2 { left: lv2::Lv2Processor, right: lv2::Lv2Processor }
impl StereoProcessor for DualMonoLv2 {
    fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.left.process_sample(input[0]), self.right.process_sample(input[1])]
    }
}

fn build_mono(sample_rate: f32, attack: f32, release: f32, threshold: f32, makeup: f32) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;
    // Connect sidechain + unused stereo ports as extras
    lv2::build_lv2_processor_with_extras(
        &lib_path, PLUGIN_URI, sample_rate as f64, &bundle_path,
        &[PORT_AUDIO_IN_L], &[PORT_AUDIO_OUT_L],
        &[(PORT_ATTACK, attack), (PORT_RELEASE, release), (PORT_THRESHOLD, threshold),
          (PORT_MAKEUP, makeup), (PORT_MAX_GATE_CLOSE, -50.0), (PORT_STEREO_LINK, 1.0)],
        &[1, 2, 3, 5], // InR, SidechainL, SidechainR, OutR
    )
}

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let attack = required_f32(params, "attack").map_err(anyhow::Error::msg)?;
    let release = required_f32(params, "release").map_err(anyhow::Error::msg)?;
    let threshold = required_f32(params, "threshold").map_err(anyhow::Error::msg)?;
    let makeup = required_f32(params, "makeup").map_err(anyhow::Error::msg)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(build_mono(sample_rate, attack, release, threshold, makeup)?))),
        AudioChannelLayout::Stereo => {
            let left = build_mono(sample_rate, attack, release, threshold, makeup)?;
            let right = build_mono(sample_rate, attack, release, threshold, makeup)?;
            Ok(BlockProcessor::Stereo(Box::new(DualMonoLv2 { left, right })))
        }
    }
}

pub const MODEL_DEFINITION: DynModelDefinition = DynModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DynBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
