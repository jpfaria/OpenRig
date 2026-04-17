use crate::registry::ModModelDefinition;
use crate::ModBackendKind;
use anyhow::Result;
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor};

pub const MODEL_ID: &str = "lv2_fomp_cs_chorus";
pub const DISPLAY_NAME: &str = "CS Chorus";
const BRAND: &str = "fomp";

const PLUGIN_URI: &str = "http://drobilla.net/plugins/fomp/cs_chorus1";
const PLUGIN_DIR: &str = "fomp";

#[cfg(target_os = "macos")]
const PLUGIN_BINARY: &str = "cs_chorus.dylib";
#[cfg(target_os = "linux")]
const PLUGIN_BINARY: &str = "cs_chorus.so";
#[cfg(target_os = "windows")]
const PLUGIN_BINARY: &str = "cs_chorus.dll";

// LV2 port indices (from TTL)
const PORT_AUDIO_IN: usize = 0;
const PORT_AUDIO_OUT: usize = 1;
const PORT_DELAY: usize = 2;
const PORT_MOD_FREQ_1: usize = 3;
const PORT_MOD_AMP_1: usize = 4;
const PORT_MOD_FREQ_2: usize = 5;
const PORT_MOD_AMP_2: usize = 6;

pub fn model_schema() -> ModelParameterSchema {
    ModelParameterSchema {
        effect_type: block_core::EFFECT_TYPE_MODULATION.into(),
        model: MODEL_ID.into(),
        display_name: DISPLAY_NAME.into(),
        audio_mode: ModelAudioMode::MonoToStereo,
        parameters: vec![
            float_parameter(
                "delay",
                "Delay",
                None,
                Some(1.0),
                0.0,
                30.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "mod_freq_1",
                "Mod Frequency 1",
                None,
                Some(0.25),
                0.003,
                10.0,
                0.001,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "mod_amp_1",
                "Mod Amplitude 1",
                None,
                Some(1.0),
                0.0,
                10.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
            float_parameter(
                "mod_freq_2",
                "Mod Frequency 2",
                None,
                Some(0.125),
                0.01,
                30.0,
                0.001,
                ParameterUnit::Hertz,
            ),
            float_parameter(
                "mod_amp_2",
                "Mod Amplitude 2",
                None,
                Some(0.5),
                0.0,
                3.0,
                0.1,
                ParameterUnit::Milliseconds,
            ),
        ],
    }
}

fn build_mono_processor(
    sample_rate: f32,
    delay: f32,
    mod_freq_1: f32,
    mod_amp_1: f32,
    mod_freq_2: f32,
    mod_amp_2: f32,
) -> Result<lv2::Lv2Processor> {
    let lib_path = lv2::resolve_lv2_lib(PLUGIN_BINARY)?;
    let bundle_path = lv2::resolve_lv2_bundle(PLUGIN_DIR)?;

    lv2::build_lv2_processor(
        &lib_path,
        PLUGIN_URI,
        sample_rate as f64,
        &bundle_path,
        &[PORT_AUDIO_IN],
        &[PORT_AUDIO_OUT],
        &[
            (PORT_DELAY, delay),
            (PORT_MOD_FREQ_1, mod_freq_1),
            (PORT_MOD_AMP_1, mod_amp_1),
            (PORT_MOD_FREQ_2, mod_freq_2),
            (PORT_MOD_AMP_2, mod_amp_2),
        ],
    )
}

fn build(
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    let delay = required_f32(params, "delay").map_err(anyhow::Error::msg)?;
    let mod_freq_1 = required_f32(params, "mod_freq_1").map_err(anyhow::Error::msg)?;
    let mod_amp_1 = required_f32(params, "mod_amp_1").map_err(anyhow::Error::msg)?;
    let mod_freq_2 = required_f32(params, "mod_freq_2").map_err(anyhow::Error::msg)?;
    let mod_amp_2 = required_f32(params, "mod_amp_2").map_err(anyhow::Error::msg)?;

    let _ = layout;
    let processor = build_mono_processor(sample_rate, delay, mod_freq_1, mod_amp_1, mod_freq_2, mod_amp_2)?;
    struct MonoAsStereo(lv2::Lv2Processor);
    impl StereoProcessor for MonoAsStereo {
        fn process_frame(&mut self, input: [f32; 2]) -> [f32; 2] {
            let out = self.0.process_sample(input[0]);
            [out, out]
        }
    }
    Ok(BlockProcessor::Stereo(Box::new(MonoAsStereo(processor))))
}

fn schema() -> Result<ModelParameterSchema> {
    Ok(model_schema())
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
