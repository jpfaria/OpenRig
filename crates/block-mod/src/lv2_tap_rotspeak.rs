// @platform: linux
use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};

pub const MODEL_ID: &str = "lv2_tap_rotspeak";
pub const DISPLAY_NAME: &str = "TAP Rotary Speaker";
const BRAND: &str = "tap";

const PLUGIN_URI: &str = "http://moddevices.com/plugins/tap/rotspeak";
const PLUGIN_DIR: &str = "tap-rotspeak";
const PLUGIN_BINARY: &str = "tap_rotspeak.so";

// LV2 port indices (from tap_rotspeak.ttl)
// Controls: 0=hornfreq, 1=bassfreq, 2=stwidht(Mic Distance), 3=hrbal(Rotor/Horn Mix)
// Output control: 4=latency (must be connected to a valid buffer)
// Audio: 5=inputl, 6=inputr, 7=outputl, 8=outputr
const PORT_HORN_FREQ: usize = 0;
const PORT_ROTOR_FREQ: usize = 1;
const PORT_MIC_DISTANCE: usize = 2;
const PORT_ROTOR_HORN_MIX: usize = 3;
const PORT_LATENCY: usize = 4; // Output control port — must be connected
const PORT_AUDIO_IN_L: usize = 5;
const PORT_AUDIO_IN_R: usize = 6;
const PORT_AUDIO_OUT_L: usize = 7;
const PORT_AUDIO_OUT_R: usize = 8;

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter("horn_hz", "Horn Freq", None, Some(0.0), 0.0, 30.0, 0.1, ParameterUnit::Hertz),
            float_parameter("rotor_hz", "Rotor Freq", None, Some(0.0), 0.0, 30.0, 0.1, ParameterUnit::Hertz),
            float_parameter("mic_distance", "Mic Distance", None, Some(0.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    })
}

fn build(params: &ParameterSet, sample_rate: f32, _layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let horn_hz = required_f32(params, "horn_hz").map_err(anyhow::Error::msg)?;
    let rotor_hz = required_f32(params, "rotor_hz").map_err(anyhow::Error::msg)?;
    let mic_distance = required_f32(params, "mic_distance").map_err(anyhow::Error::msg)?;

    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    // PORT_LATENCY is an output control port. It must be connected to a valid
    // writable buffer to avoid SIGSEGV. We include it in the control_ports list
    // so StereoLv2Processor connects it to a dedicated f32 in control_values.
    let processor = lv2::build_stereo_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN_L, PORT_AUDIO_IN_R],
        &[PORT_AUDIO_OUT_L, PORT_AUDIO_OUT_R],
        &[
            (PORT_HORN_FREQ, horn_hz),
            (PORT_ROTOR_FREQ, rotor_hz),
            (PORT_MIC_DISTANCE, mic_distance),
            (PORT_ROTOR_HORN_MIX, 0.5),
            (PORT_LATENCY, 0.0), // output port — plugin writes here, we just provide valid memory
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
