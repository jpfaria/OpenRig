//! Dub auto-wah variant — slow attack/release, wider sweep, lower Q.
//! The breathing reggae/dub envelope-filter voice. Same engine as
//! `native_auto_wah.rs`; only the hidden tuning differs.

use anyhow::{Error, Result};
use block_core::param::{
    float_parameter, required_f32, ModelParameterSchema, ParameterSet, ParameterUnit,
};
use block_core::{
    AudioChannelLayout, BlockProcessor, ModelAudioMode, MonoProcessor, StereoProcessor,
};

use crate::registry::native_auto_wah::{AutoWah, AutoWahParams, AutoWahTuning};
use crate::registry::WahModelDefinition;
use crate::WahBackendKind;

pub const MODEL_ID: &str = "auto_wah_dub";
pub const DISPLAY_NAME: &str = "Auto-Wah (Dub)";

const TUNING: AutoWahTuning = AutoWahTuning {
    min_cutoff_hz: 150.0,
    max_cutoff_hz: 1_600.0,
    q: 4.0,
    attack_ms: 25.0,
    release_ms: 250.0,
    sensitivity: 0.8,
};

fn schema() -> Result<ModelParameterSchema> {
    Ok(ModelParameterSchema {
        effect_type: "wah".to_string(),
        model: MODEL_ID.to_string(),
        display_name: DISPLAY_NAME.to_string(),
        audio_mode: ModelAudioMode::DualMono,
        parameters: vec![
            float_parameter("sensitivity", "Sensitivity", Some("Wah"), Some(50.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("range", "Range", Some("Wah"), Some(85.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("q", "Q", Some("Wah"), Some(40.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
            float_parameter("mix", "Mix", Some("Output"), Some(100.0), 0.0, 100.0, 1.0, ParameterUnit::Percent),
        ],
    })
}

fn parse(params: &ParameterSet) -> Result<AutoWahParams> {
    Ok(AutoWahParams {
        sensitivity: 0.25 + (required_f32(params, "sensitivity").map_err(Error::msg)? / 100.0) * 3.75,
        range: required_f32(params, "range").map_err(Error::msg)? / 100.0,
        q: 0.3 + (required_f32(params, "q").map_err(Error::msg)? / 100.0) * 1.7,
        mix: required_f32(params, "mix").map_err(Error::msg)? / 100.0,
    })
}

fn validate(params: &ParameterSet) -> Result<()> { let _ = parse(params)?; Ok(()) }

fn build(params: &ParameterSet, sample_rate: f32, layout: AudioChannelLayout) -> Result<BlockProcessor> {
    let p = parse(params)?;
    match layout {
        AudioChannelLayout::Mono => Ok(BlockProcessor::Mono(Box::new(AutoWah::with_tuning(p, sample_rate, TUNING)))),
        AudioChannelLayout::Stereo => {
            struct Dual { l: Box<dyn MonoProcessor>, r: Box<dyn MonoProcessor> }
            impl StereoProcessor for Dual {
                fn process_frame(&mut self, i: [f32; 2]) -> [f32; 2] { [self.l.process_sample(i[0]), self.r.process_sample(i[1])] }
            }
            Ok(BlockProcessor::Stereo(Box::new(Dual {
                l: Box::new(AutoWah::with_tuning(p, sample_rate, TUNING)),
                r: Box::new(AutoWah::with_tuning(p, sample_rate, TUNING)),
            })))
        }
    }
}

pub const MODEL_DEFINITION: WahModelDefinition = WahModelDefinition {
    id: MODEL_ID,
    display_name: DISPLAY_NAME,
    brand: block_core::BRAND_NATIVE,
    backend_kind: WahBackendKind::Native,
    schema,
    validate,
    build,
    supported_instruments: block_core::GUITAR_BASS,
    knob_layout: &[],
};
