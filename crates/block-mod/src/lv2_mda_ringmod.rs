// @platform: linux
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_mda_ringmod";
pub const DISPLAY_NAME: &str = "MDA RingMod";
const BRAND: &str = "mda";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/mda/RingMod";
const PLUGIN_DIR: &str = "mod-mda-RingMod";
const PLUGIN_BINARY: &str = "RingMod.so";

// LV2 port indices (from RingMod.ttl)
// Controls: 0=freq, 1=fine, 2=feedback
// Audio: 3=left_in, 4=right_in, 5=left_out, 6=right_out
const PORT_FREQ: usize = 0;
const PORT_FINE: usize = 1;
const PORT_FEEDBACK: usize = 2;
const PORT_AUDIO_IN_L: usize = 3;
const PORT_AUDIO_IN_R: usize = 4;
const PORT_AUDIO_OUT_L: usize = 5;
const PORT_AUDIO_OUT_R: usize = 6;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("freq", "Freq", None, Some(1000.0), 0.1, 16000.0, 1.0, ParameterUnit::Hertz),
            float_parameter("fine", "Fine", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Hertz),
            float_parameter("feedback", "Feedback", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    })
}

fn build(params: &ParameterSet, sample_rate: f32, _layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let freq = required_f32(params, "freq").map_err(anyhow::Error::msg)?;
    let fine = required_f32(params, "fine").map_err(anyhow::Error::msg)?;
    let feedback = required_f32(params, "feedback").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    let processor = lv2::build_stereo_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
        &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        &[
            (PORT_FREQ, freq),
            (PORT_FINE, fine),
            (PORT_FEEDBACK, feedback),
        ],
    )?;
    Ok(BlockProcessor::Stereo(Box::new(processor)))
}

pub const MODEL_DEFINITION: ModModelDefinition = ModModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: BRAND,
    backend_kind: ModBackendKind::Lv2,
    schema,
    build,
    supported_instruments: block_core::ALL_INSTRUMENTS,
    knob_layout: &[],
};
