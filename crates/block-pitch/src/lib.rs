//! Pitch correction block implementations.
mod registry;

use anyhow::Result;
use block_core::param::{ModelParameterSchema, ParameterSet};
use block_core::{AudioChannelLayout, BlockProcessor, ModelVisualData};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum PitchBackendKind {
    Native,
    Nam,
    Ir,
    Lv2,
}

pub fn supported_models() -> &'static [&'static str] {
    registry::SUPPORTED_MODELS
}

pub fn pitch_model_visual(model_id: &str) -> Option<ModelVisualData> {
    let def = registry::find_model_definition(model_id).ok()?;
    Some(ModelVisualData {
        brand: def.brand,
        type_label: match def.backend_kind {
            PitchBackendKind::Native => "NATIVE",
            PitchBackendKind::Nam => "NAM",
            PitchBackendKind::Ir => "IR",
            PitchBackendKind::Lv2 => "LV2",
        },
        supported_instruments: def.supported_instruments,
        knob_layout: def.knob_layout,
    })
}

pub fn pitch_display_name(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.display_name)
        .unwrap_or("")
}

pub fn pitch_brand(model: &str) -> &'static str {
    registry::find_model_definition(model)
        .map(|d| d.brand)
        .unwrap_or("")
}

pub fn pitch_type_label(model: &str) -> &'static str {
    pitch_model_visual(model)
        .map(|v| v.type_label)
        .unwrap_or("")
}

pub fn pitch_model_schema(model: &str) -> Result<ModelParameterSchema> {
    (registry::find_model_definition(model)?.schema)()
}

pub fn validate_pitch_params(model: &str, params: &ParameterSet) -> Result<()> {
    let schema = pitch_model_schema(model)?;
    params
        .normalized_against(&schema)
        .map(|_| ())
        .map_err(|error| anyhow::anyhow!(error))
}

pub fn build_pitch_processor(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
) -> Result<BlockProcessor> {
    build_pitch_processor_for_layout(model, params, sample_rate, AudioChannelLayout::Stereo)
}

pub fn build_pitch_processor_for_layout(
    model: &str,
    params: &ParameterSet,
    sample_rate: f32,
    layout: AudioChannelLayout,
) -> Result<BlockProcessor> {
    (registry::find_model_definition(model)?.build)(params, sample_rate, layout)
}

#[cfg(test)]
mod tests {
    use super::{
        build_pitch_processor_for_layout, pitch_brand, pitch_display_name, pitch_model_schema,
        pitch_type_label, supported_models, validate_pitch_params,
    };
    use block_core::param::ParameterSet;
    use block_core::AudioChannelLayout;

    // ── helpers ──────────────────────────────────────────────────────────

    fn defaults_for(model: &str) -> ParameterSet {
        let schema = pitch_model_schema(model).expect("schema");
        ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize")
    }

    // ── registry-wide tests ─────────────────────────────────────────────

    #[test]
    fn registry_schema_all_models_return_non_empty_schema() {
        for model in supported_models() {
            let schema = pitch_model_schema(model)
                .unwrap_or_else(|e| panic!("schema() failed for '{model}': {e}"));
            assert_eq!(schema.model, *model, "schema.model mismatch for '{model}'");
            assert_eq!(
                schema.effect_type, "pitch",
                "effect_type mismatch for '{model}'"
            );
        }
    }

    #[test]
    fn registry_validate_all_models_accept_empty_params() {
        for model in supported_models() {
            // Use empty ParameterSet because validate_pitch_params internally
            // calls normalized_against which fills defaults without re-validating them.
            validate_pitch_params(model, &ParameterSet::default())
                .unwrap_or_else(|e| panic!("validate() rejected empty params for '{model}': {e}"));
        }
    }

    #[test]
    fn registry_metadata_all_models_have_display_name_and_brand() {
        for model in supported_models() {
            let name = pitch_display_name(model);
            assert!(!name.is_empty(), "display_name empty for '{model}'");
            let brand = pitch_brand(model);
            assert!(!brand.is_empty(), "brand empty for '{model}'");
            let label = pitch_type_label(model);
            assert!(!label.is_empty(), "type_label empty for '{model}'");
        }
    }

    #[test]
    fn registry_schema_defaults_normalize_for_all_models() {
        for model in supported_models() {
            let schema = pitch_model_schema(model).expect("schema");
            let result = ParameterSet::default().normalized_against(&schema);
            assert!(
                result.is_ok(),
                "defaults failed to normalize for '{model}': {}",
                result.unwrap_err()
            );
        }
    }

    // ── LV2 models: build requires external plugins, skip ───────────

    #[test]
    #[ignore]
    fn registry_build_lv2_models_ignored() {
        for model in supported_models() {
            let params = defaults_for(model);
            let _ = build_pitch_processor_for_layout(
                model,
                &params,
                48_000.0,
                AudioChannelLayout::Mono,
            );
        }
    }

    // ── existing specific tests (kept) ──────────────────────────────────

    #[test]
    fn exposes_x42_autotune() {
        let models = supported_models();
        assert!(
            models.contains(&"lv2_fat1_autotune"),
            "should contain x42 autotune"
        );
    }

    #[test]
    fn x42_schema_is_pitch() {
        let schema = pitch_model_schema("lv2_fat1_autotune").expect("schema");
        assert_eq!(schema.effect_type, "pitch");
        assert_eq!(schema.model, "lv2_fat1_autotune");
    }

    #[test]
    fn defaults_normalize_x42() {
        let params = ParameterSet::default();
        validate_pitch_params("lv2_fat1_autotune", &params).expect("defaults should normalize");
    }
}

pub fn is_pitch_model_available(model: &str) -> bool {
    registry::is_model_available(model)
}
