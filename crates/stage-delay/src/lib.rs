//! Delay implementations.
pub mod analog_warm;
pub mod digital_clean;
pub mod modulated_delay;
pub mod reverse;
pub mod shared;
pub mod slapback;
pub mod tape_vintage;

use analog_warm::{
    build_mono_processor as build_analog_warm_processor,
    model_schema as analog_warm_model_schema, supports_model as supports_analog_warm_model,
};
use anyhow::{bail, Result};
use digital_clean::{
    build_mono_processor as build_digital_clean_processor,
    model_schema as digital_clean_model_schema, supports_model as supports_digital_clean_model,
};
use modulated_delay::{
    build_mono_processor as build_modulated_delay_processor,
    model_schema as modulated_delay_model_schema,
    supports_model as supports_modulated_delay_model,
};
use reverse::{
    build_mono_processor as build_reverse_processor, model_schema as reverse_model_schema,
    supports_model as supports_reverse_model,
};
use shared::build_dual_mono_from_builder;
use slapback::{
    build_mono_processor as build_slapback_processor, model_schema as slapback_model_schema,
    supports_model as supports_slapback_model,
};
use stage_core::param::{ModelParameterSchema, ParameterSet};
use stage_core::{AudioChannelLayout, NamedModel, StageProcessor};
use tape_vintage::{
    build_mono_processor as build_tape_vintage_processor,
    model_schema as tape_vintage_model_schema, supports_model as supports_tape_vintage_model,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayModel {
    DigitalClean,
    AnalogWarm,
    TapeVintage,
    Reverse,
    Slapback,
    ModulatedDelay,
}

impl NamedModel for DelayModel {
    fn model_key(&self) -> &'static str {
        match self {
            DelayModel::DigitalClean => digital_clean::MODEL_ID,
            DelayModel::AnalogWarm => analog_warm::MODEL_ID,
            DelayModel::TapeVintage => tape_vintage::MODEL_ID,
            DelayModel::Reverse => reverse::MODEL_ID,
            DelayModel::Slapback => slapback::MODEL_ID,
            DelayModel::ModulatedDelay => modulated_delay::MODEL_ID,
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            DelayModel::DigitalClean => "Digital Clean Delay",
            DelayModel::AnalogWarm => "Analog Warm Delay",
            DelayModel::TapeVintage => "Tape Vintage Delay",
            DelayModel::Reverse => "Reverse Delay",
            DelayModel::Slapback => "Slapback Delay",
            DelayModel::ModulatedDelay => "Modulated Delay",
        }
    }
}

pub fn delay_model_schema(model: &str) -> Result<ModelParameterSchema> {
    if supports_digital_clean_model(model) {
        Ok(digital_clean_model_schema())
    } else if supports_analog_warm_model(model) {
        Ok(analog_warm_model_schema())
    } else if supports_tape_vintage_model(model) {
        Ok(tape_vintage_model_schema())
    } else if supports_reverse_model(model) {
        Ok(reverse_model_schema())
    } else if supports_slapback_model(model) {
        Ok(slapback_model_schema())
    } else if supports_modulated_delay_model(model) {
        Ok(modulated_delay_model_schema())
    } else {
        bail!("unsupported delay model '{}'", model)
    }
}

pub fn build_delay_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<StageProcessor> {
    build_delay_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_delay_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<StageProcessor> {
    if supports_digital_clean_model(model) {
        build_dual_mono_delay_processor(layout, || build_digital_clean_processor(params, sample_rate))
    } else if supports_analog_warm_model(model) {
        build_dual_mono_delay_processor(layout, || build_analog_warm_processor(params, sample_rate))
    } else if supports_tape_vintage_model(model) {
        build_dual_mono_delay_processor(layout, || build_tape_vintage_processor(params, sample_rate))
    } else if supports_reverse_model(model) {
        build_dual_mono_delay_processor(layout, || build_reverse_processor(params, sample_rate))
    } else if supports_slapback_model(model) {
        build_dual_mono_delay_processor(layout, || build_slapback_processor(params, sample_rate))
    } else if supports_modulated_delay_model(model) {
        build_dual_mono_delay_processor(layout, || build_modulated_delay_processor(params, sample_rate))
    } else {
        bail!("unsupported delay model '{}'", model)
    }
}

fn build_dual_mono_delay_processor<F>(
    layout: AudioChannelLayout,
    builder: F,
) -> Result<StageProcessor>
where
    F: Fn() -> Result<Box<dyn stage_core::MonoProcessor>>,
{
    match layout {
        AudioChannelLayout::Mono => Ok(StageProcessor::Mono(builder()?)),
        AudioChannelLayout::Stereo => {
            Ok(StageProcessor::Stereo(build_dual_mono_from_builder(builder)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_delay_processor_for_layout, delay_model_schema};
    use stage_core::param::ParameterSet;
    use stage_core::AudioChannelLayout;

    #[test]
    fn new_delay_catalog_is_publicly_supported() {
        for model in [
            "digital_clean",
            "analog_warm",
            "tape_vintage",
            "reverse",
            "slapback",
            "modulated_delay",
        ] {
            assert!(
                delay_model_schema(model).is_ok(),
                "expected '{model}' to be supported"
            );
        }
    }

    #[test]
    fn legacy_delay_models_are_rejected() {
        for model in ["digital_basic", "digital_ping_pong", "digital_wide"] {
            assert!(
                delay_model_schema(model).is_err(),
                "expected legacy model '{model}' to be rejected"
            );
        }
    }

    #[test]
    fn digital_clean_builds_for_stereo_tracks() {
        let schema = delay_model_schema("digital_clean").expect("schema");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("normalized defaults");
        let processor = build_delay_processor_for_layout(
            "digital_clean",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        );

        assert!(processor.is_ok(), "digital_clean should accept stereo tracks");
    }
}
