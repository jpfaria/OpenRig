//! Gain blocks such as boost, overdrive, distortion, and fuzz.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GainBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn gain_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            GainBackendKind::Native => "NATIVE",
            GainBackendKind::Nam => "NAM",
            GainBackendKind::Ir => "IR",
            GainBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn gain_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.display_name).unwrap_or("")
}

pub fn gain_brand(model: &str) -> &'static str {
    registry::find_model_definition(model).map(|d| d.brand).unwrap_or("")
}

pub fn gain_type_label(model: &str) -> &'static str {
    gain_model_visual(model).map(|v| v.type_label).unwrap_or("")
}

pub fn gain_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn gain_asset_summary(model: &str, params: &ParameterSet) -> Result<String> {
    (registry::find_model_definition(model)?.asset_summary)(params)
}

pub fn validate_gain_params(model: &str, params: &ParameterSet) -> Result<()> {
    (registry::find_model_definition(model)?.validate)(params)
}

pub fn build_gain_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_gain_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Mono)
}

pub fn build_gain_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{build_gain_processor_for_layout, gain_model_schema, supported_models};
    use block_core::param::ParameterSet;
    use block_core::{AudioChannelLayout, BlockProcessor, ModelAudioMode};
    use domain::value_objects::ParameterValue;

    #[test]
    fn supported_gain_models_expose_valid_schema() {
        for model in supported_models() {
            let schema = gain_model_schema(model).expect("schema should exist");
            assert_eq!(schema.model, *model);
            assert_eq!(schema.effect_type, "gain");
            assert!(
                !schema.parameters.is_empty(),
                "model '{model}' should expose parameters"
            );
        }
    }

    #[test]
    fn ibanez_ts9_schema_exposes_drive_tone_and_level() {
        let schema = gain_model_schema("ibanez_ts9").expect("ts9 schema should exist");

        assert_eq!(schema.effect_type, "gain");
        assert_eq!(schema.model, "ibanez_ts9");
        assert_eq!(schema.audio_mode, ModelAudioMode::DualMono);
        assert_eq!(
            schema
                .parameters
                .iter()
                .map(|parameter| parameter.path.as_str())
                .collect::<Vec<_>>(),
            vec!["drive", "tone", "level"]
        );
    }

    #[test]
    fn ibanez_ts9_builds_for_mono_and_stereo_layouts() {
        let schema = gain_model_schema("ibanez_ts9").expect("ts9 schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");

        let mono = build_gain_processor_for_layout(
            "ibanez_ts9",
            &params,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("mono ts9 should build");
        assert!(matches!(mono, BlockProcessor::Mono(_)));

        let stereo = build_gain_processor_for_layout(
            "ibanez_ts9",
            &params,
            48_000.0,
            AudioChannelLayout::Stereo,
        )
        .expect("stereo ts9 should build");
        assert!(matches!(stereo, BlockProcessor::Stereo(_)));
    }

    #[test]
    fn ibanez_ts9_level_changes_output_gain() {
        let schema = gain_model_schema("ibanez_ts9").expect("ts9 schema should exist");

        let mut quiet = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        quiet.insert("drive", ParameterValue::Float(35.0));
        quiet.insert("tone", ParameterValue::Float(50.0));
        quiet.insert("level", ParameterValue::Float(20.0));

        let mut loud = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        loud.insert("drive", ParameterValue::Float(35.0));
        loud.insert("tone", ParameterValue::Float(50.0));
        loud.insert("level", ParameterValue::Float(80.0));

        let mut quiet_processor = match build_gain_processor_for_layout(
            "ibanez_ts9",
            &quiet,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("quiet ts9 should build")
        {
            BlockProcessor::Mono(processor) => processor,
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        };

        let mut loud_processor = match build_gain_processor_for_layout(
            "ibanez_ts9",
            &loud,
            48_000.0,
            AudioChannelLayout::Mono,
        )
        .expect("loud ts9 should build")
        {
            BlockProcessor::Mono(processor) => processor,
            BlockProcessor::Stereo(_) => panic!("expected mono processor"),
        };

        let quiet_output = quiet_processor.process_sample(0.2).abs();
        let loud_output = loud_processor.process_sample(0.2).abs();

        assert!(
            loud_output > quiet_output,
            "level should raise output: quiet={quiet_output}, loud={loud_output}"
        );
    }
}
