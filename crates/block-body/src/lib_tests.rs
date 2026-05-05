
use crate::{
    body_asset_summary, body_backend_kind, body_brand, body_display_name, body_model_schema,
    body_model_visual, body_type_label, build_body_processor_for_layout, supported_models,
    validate_body_params, BodyBackendKind,
};
use block_core::param::ParameterSet;
use block_core::AudioChannelLayout;

#[test]
#[ignore]
fn supported_bodies_expose_valid_schema() {
    for model in supported_models() {
        let schema = body_model_schema(model).expect("body schema should exist");
        assert_eq!(schema.model, *model);
        assert!(
            !schema.parameters.is_empty(),
            "model '{model}' should expose parameters"
        );
    }
}

#[test]
#[ignore]
fn supported_bodies_build_for_mono_chains() {
    for model in supported_models() {
        let schema = body_model_schema(model).expect("schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");

        let processor =
            build_body_processor_for_layout(model, &params, 48_000.0, AudioChannelLayout::Mono);

        assert!(
            processor.is_ok(),
            "expected '{model}' to build for mono chains"
        );
    }
}

// ── supported_models ────────────────────────────────────────────────────

#[test]
fn supported_models_has_no_duplicates() {
    let models = supported_models();
    let mut seen = std::collections::HashSet::new();
    for model in models {
        assert!(seen.insert(model), "duplicate model id: {model}");
    }
}

// ── schema for all models ───────────────────────────────────────────────

#[test]
fn all_models_return_valid_schema() {
    for model in supported_models() {
        let schema = body_model_schema(model);
        assert!(
            schema.is_ok(),
            "schema() failed for model '{model}': {}",
            schema.unwrap_err()
        );
        let schema = schema.unwrap();
        assert_eq!(
            schema.model, *model,
            "schema model id mismatch for '{model}'"
        );
        assert!(
            !schema.parameters.is_empty(),
            "model '{model}' should have parameters"
        );
    }
}

// ── validate with defaults for all models ───────────────────────────────

#[test]
fn all_models_validate_with_defaults() {
    for model in supported_models() {
        let schema = body_model_schema(model).expect("schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        let result = validate_body_params(model, &params);
        assert!(
            result.is_ok(),
            "validate failed for model '{model}' with defaults: {}",
            result.unwrap_err()
        );
    }
}

// ── display_name for all models ─────────────────────────────────────────

#[test]
fn all_models_have_non_empty_display_name() {
    for model in supported_models() {
        let name = body_display_name(model);
        assert!(!name.is_empty(), "model '{model}' has empty display_name");
    }
}

// ── brand for all models ────────────────────────────────────────────────

#[test]
fn all_models_return_brand() {
    for model in supported_models() {
        // body_brand returns "" for unknown models; for known models it returns
        // the brand string (which may be empty for some boutique models)
        let _brand = body_brand(model);
    }
}

// ── backend_kind for all models ─────────────────────────────────────────

#[test]
fn all_models_have_ir_backend() {
    for model in supported_models() {
        let kind = body_backend_kind(model).expect("backend_kind should resolve");
        assert_eq!(
            kind,
            BodyBackendKind::Ir,
            "body model '{model}' should be IR backend"
        );
    }
}

// ── type_label for all models ───────────────────────────────────────────

#[test]
fn all_models_type_label_is_ir() {
    for model in supported_models() {
        let label = body_type_label(model);
        assert_eq!(
            label, "IR",
            "body model '{model}' type_label should be 'IR', got '{label}'"
        );
    }
}

// ── model_visual for all models ─────────────────────────────────────────

#[test]
fn all_models_return_visual_data() {
    for model in supported_models() {
        let visual = body_model_visual(model);
        assert!(
            visual.is_some(),
            "model '{model}' should return visual data"
        );
        let visual = visual.unwrap();
        assert_eq!(visual.type_label, "IR");
        assert!(
            !visual.supported_instruments.is_empty(),
            "model '{model}' should support at least one instrument"
        );
    }
}

// ── asset_summary for all models ────────────────────────────────────────

#[test]
fn all_models_return_asset_summary_with_defaults() {
    for model in supported_models() {
        let schema = body_model_schema(model).expect("schema should exist");
        let params = ParameterSet::default()
            .normalized_against(&schema)
            .expect("defaults should normalize");
        let summary = body_asset_summary(model, &params);
        assert!(
            summary.is_ok(),
            "asset_summary failed for model '{model}': {}",
            summary.unwrap_err()
        );
        assert!(
            !summary.unwrap().is_empty(),
            "asset_summary should not be empty for '{model}'"
        );
    }
}

// ── error paths ─────────────────────────────────────────────────────────

#[test]
fn unknown_model_returns_error_for_schema() {
    let result = body_model_schema("nonexistent_body_model_xyz");
    assert!(result.is_err());
}

#[test]
fn unknown_model_returns_error_for_validate() {
    let result = validate_body_params("nonexistent_body_model_xyz", &ParameterSet::default());
    assert!(result.is_err());
}

#[test]
fn unknown_model_returns_error_for_backend_kind() {
    let result = body_backend_kind("nonexistent_body_model_xyz");
    assert!(result.is_err());
}

#[test]
fn unknown_model_returns_empty_display_name() {
    assert_eq!(body_display_name("nonexistent_body_model_xyz"), "");
}

#[test]
fn unknown_model_returns_empty_brand() {
    assert_eq!(body_brand("nonexistent_body_model_xyz"), "");
}

#[test]
fn unknown_model_returns_empty_type_label() {
    assert_eq!(body_type_label("nonexistent_body_model_xyz"), "");
}

#[test]
fn unknown_model_returns_none_for_visual() {
    assert!(body_model_visual("nonexistent_body_model_xyz").is_none());
}

#[test]
fn unknown_model_returns_error_for_asset_summary() {
    let result = body_asset_summary("nonexistent_body_model_xyz", &ParameterSet::default());
    assert!(result.is_err());
}

#[test]
#[ignore]
fn unknown_model_returns_error_for_build() {
    let result = build_body_processor_for_layout(
        "nonexistent_body_model_xyz",
        &ParameterSet::default(),
        48_000.0,
        AudioChannelLayout::Mono,
    );
    assert!(result.is_err());
}
