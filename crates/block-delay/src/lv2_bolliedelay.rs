use crate::registry::DelayModelDefinition;
use crate::DelayBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor};

pub const MODEL_ID: &str = "lv2_bolliedelay";
pub const DISPLAY_NAME: &str = "Bollie Delay";
const BRAND: &str = "bollie";

const PLUGIN_URI: &str = "https://ca9.eu/lv2/bolliedelay";
const PLUGIN_DIR: &str = "bolliedelay";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "bolliedelay.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "bolliedelay.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "bolliedelay.dll";

// LV2 port indices (from TTL) — stereo in/out
const PORT_TEMPO_HOST: usize = 0;
const PORT_TEMPO_USER: usize = 1;
const PORT_TEMPO_MODE: usize = 2;
const PORT_TAP: usize = 3;
const PORT_MIX: usize = 4;
const PORT_FEEDBACK: usize = 5;
const PORT_CROSSF: usize = 6;
const PORT_LOW_ON: usize = 7;
const PORT_LOW_F: usize = 8;
const PORT_LOW_Q: usize = 9;
const PORT_HIGH_ON: usize = 10;
const PORT_HIGH_F: usize = 11;
const PORT_HIGH_Q: usize = 12;
const PORT_DIV_L: usize = 13;
const PORT_DIV_R: usize = 14;
const PORT_AUDIO_IN_L: usize = 15;
const PORT_AUDIO_IN_R: usize = 16;
const PORT_AUDIO_OUT_L: usize = 17;
const PORT_AUDIO_OUT_R: usize = 18;
// PORT 19 = output (skipped)

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_DELAY.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("tempo_user", "Tempo", None, Some(120.0), 6.0, 1000.0, 1.0, ParameterUnit::None),
            float_parameter("mix", "Mix", None, Some(30.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("feedback", "Feedback", None, Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("crossf", "Crossfeed", None, Some(20.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    }
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let tempo_user = required_f32(params, "tempo_user").map_err(anyhow::Error::msg)?;
    let mix = required_f32(params, "mix").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)?;
    let crossf = required_f32(params, "crossf").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let control_ports = &[
        // Skipped ports with fixed defaults
        (PORT_TEMPO_HOST, 120.0), (PORT_TEMPO_MODE, 0.0), (PORT_TAP, 0.0),
        // Exposed parameters
        (PORT_TEMPO_USER, tempo_user), (PORT_MIX, mix),
        (PORT_FEEDBACK, feedback), (PORT_CROSSF, crossf),
        // EQ ports — fixed defaults (off)
        (PORT_LOW_ON, 0.0), (PORT_LOW_F, 20.0), (PORT_LOW_Q, 1.0),
        (PORT_HIGH_ON, 0.0), (PORT_HIGH_F, 7500.0), (PORT_HIGH_Q, 1.0),
        // Division ports — fixed defaults
        (PORT_DIV_L, 0.0), (PORT_DIV_R, 0.0),
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

struct StereoAsMono(lv2::StereoLv2Processor);
impl MonoProcessor for StereoAsMono {
    fn process_sample(&mut self, input: f32) -> f32 {
        let [l, r] = block_core::StereoProcessor::process_frame(&mut self.0, [input, input]);
        (l + r) * 0.5
    }
}

fn schema() -> Result<ModelParameterSchema> { Ok(model_schema()) }

pub const MODEL_DEFINITION: DelayModelDefinition = DelayModelDefinition {
    id: MODEL_ID, display_name: DISPLAY_NAME, brand: BRAND,
    backend_kind: DelayBackendKind::Lv2, schema, build,
    supported_instruments: block_core::ALL_INSTRUMENTS, knob_layout: &[],
};
