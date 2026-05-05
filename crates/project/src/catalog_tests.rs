//! Tests for `project::catalog`. Lifted from `catalog.rs` so the production
//! file stays under the size cap. Re-attached via `#[cfg(test)] #[path] mod tests;`.

use super::{supported_block_models, supported_block_types};

#[test]
fn catalog_exposes_supported_types() {
    let effect_types = supported_block_types()
        .into_iter()
        .map(|entry| entry.effect_type)
        .collect::<Vec<_>>();

    assert!(effect_types.contains(&"preamp"));
    assert!(effect_types.contains(&"delay"));
    assert!(effect_types.contains(&"nam"));
    assert!(effect_types.contains(&"ir"));
    assert!(effect_types.contains(&"wah"));
    assert!(effect_types.contains(&"pitch"));
}

#[test]
fn catalog_mirrors_core_supported_models() {
    let amp_model_ids = supported_block_models("preamp")
        .expect("preamp catalog")
        .into_iter()
        .map(|entry| entry.model_id)
        .collect::<Vec<_>>();
    let expected = block_preamp::supported_models()
        .iter()
        .map(|model| (*model).to_string())
        .collect::<Vec<_>>();

    assert_eq!(amp_model_ids, expected);

    let delay_model_ids = supported_block_models("delay")
        .expect("delay catalog")
        .into_iter()
        .map(|entry| entry.model_id)
        .collect::<Vec<_>>();
    let expected = block_delay::supported_models()
        .iter()
        .map(|model| (*model).to_string())
        .collect::<Vec<_>>();

    assert_eq!(delay_model_ids, expected);
}

// --- model_display_name tests ---

#[test]
fn model_display_name_known_preamp_returns_nonempty() {
    let models = block_preamp::supported_models();
    let name = super::model_display_name("preamp", models[0]);
    assert!(
        !name.is_empty(),
        "display_name for known preamp should be non-empty"
    );
}

#[test]
fn model_display_name_unknown_type_returns_empty() {
    let name = super::model_display_name("nonexistent", "some_model");
    assert_eq!(name, "");
}

#[test]
fn model_display_name_unknown_model_returns_empty() {
    let name = super::model_display_name("preamp", "nonexistent_model_xyz");
    assert_eq!(name, "");
}

#[test]
fn model_display_name_all_effect_types_known_model() {
    let type_model_pairs: Vec<(&str, &str)> = vec![
        ("delay", block_delay::supported_models()[0]),
        ("reverb", block_reverb::supported_models()[0]),
        ("gain", block_gain::supported_models()[0]),
        ("dynamics", block_dyn::supported_models()[0]),
        ("filter", block_filter::supported_models()[0]),
        ("wah", block_wah::supported_models()[0]),
        ("pitch", block_pitch::supported_models()[0]),
        ("modulation", block_mod::supported_models()[0]),
        ("amp", block_amp::supported_models()[0]),
        ("cab", block_cab::supported_models()[0]),
        ("body", block_body::supported_models()[0]),
        ("ir", block_ir::supported_models()[0]),
        ("nam", block_nam::supported_models()[0]),
    ];
    for (effect_type, model_id) in type_model_pairs {
        let name = super::model_display_name(effect_type, model_id);
        assert!(
            !name.is_empty(),
            "display_name for {effect_type}:{model_id} should be non-empty"
        );
    }
}

// --- model_brand tests ---

#[test]
fn model_brand_known_preamp_returns_string() {
    let models = block_preamp::supported_models();
    let brand = super::model_brand("preamp", models[0]);
    // brand can be empty for some models, but shouldn't panic
    let _ = brand;
}

#[test]
fn model_brand_unknown_type_returns_empty() {
    let brand = super::model_brand("nonexistent", "some_model");
    assert_eq!(brand, "");
}

#[test]
fn model_brand_all_effect_types() {
    let type_model_pairs: Vec<(&str, &str)> = vec![
        ("delay", block_delay::supported_models()[0]),
        ("reverb", block_reverb::supported_models()[0]),
        ("gain", block_gain::supported_models()[0]),
        ("dynamics", block_dyn::supported_models()[0]),
        ("filter", block_filter::supported_models()[0]),
        ("wah", block_wah::supported_models()[0]),
        ("pitch", block_pitch::supported_models()[0]),
        ("modulation", block_mod::supported_models()[0]),
        ("amp", block_amp::supported_models()[0]),
        ("cab", block_cab::supported_models()[0]),
        ("body", block_body::supported_models()[0]),
        ("ir", block_ir::supported_models()[0]),
        ("nam", block_nam::supported_models()[0]),
    ];
    for (effect_type, model_id) in type_model_pairs {
        // Should not panic for any known effect type
        let _ = super::model_brand(effect_type, model_id);
    }
}

// --- model_type_label tests ---

#[test]
fn model_type_label_known_preamp_returns_nonempty() {
    let models = block_preamp::supported_models();
    let label = super::model_type_label("preamp", models[0]);
    assert!(
        !label.is_empty(),
        "type_label for known preamp should be non-empty"
    );
}

#[test]
fn model_type_label_unknown_type_returns_empty() {
    let label = super::model_type_label("nonexistent", "some_model");
    assert_eq!(label, "");
}

#[test]
fn model_type_label_all_effect_types() {
    let type_model_pairs: Vec<(&str, &str)> = vec![
        ("delay", block_delay::supported_models()[0]),
        ("reverb", block_reverb::supported_models()[0]),
        ("gain", block_gain::supported_models()[0]),
        ("dynamics", block_dyn::supported_models()[0]),
        ("filter", block_filter::supported_models()[0]),
        ("wah", block_wah::supported_models()[0]),
        ("pitch", block_pitch::supported_models()[0]),
        ("modulation", block_mod::supported_models()[0]),
        ("amp", block_amp::supported_models()[0]),
        ("cab", block_cab::supported_models()[0]),
        ("body", block_body::supported_models()[0]),
        ("ir", block_ir::supported_models()[0]),
        ("nam", block_nam::supported_models()[0]),
    ];
    for (effect_type, model_id) in type_model_pairs {
        let label = super::model_type_label(effect_type, model_id);
        assert!(
            !label.is_empty(),
            "type_label for {effect_type}:{model_id} should be non-empty"
        );
    }
}

// --- block_has_external_gui tests ---

#[test]
fn block_has_external_gui_vst3_returns_true() {
    assert!(super::block_has_external_gui("vst3"));
}

#[test]
fn block_has_external_gui_non_vst3_returns_false() {
    let non_vst3_types = [
        "preamp",
        "amp",
        "cab",
        "delay",
        "reverb",
        "gain",
        "dynamics",
        "filter",
        "wah",
        "pitch",
        "modulation",
        "utility",
        "body",
        "ir",
        "nam",
        "full_rig",
    ];
    for effect_type in non_vst3_types {
        assert!(
            !super::block_has_external_gui(effect_type),
            "{effect_type} should not have external GUI"
        );
    }
}

// --- supported_block_models for all effect types ---

#[test]
fn supported_block_models_all_registered_types() {
    let registered_types = supported_block_types()
        .into_iter()
        .map(|entry| entry.effect_type)
        .collect::<Vec<_>>();

    for effect_type in registered_types {
        if effect_type == "vst3" {
            continue; // VST3 depends on runtime discovery
        }
        let models = supported_block_models(effect_type)
            .unwrap_or_else(|e| panic!("supported_block_models({effect_type}) failed: {e}"));
        assert!(
            !models.is_empty(),
            "{effect_type} should have at least one model"
        );
        for model in &models {
            assert!(!model.model_id.is_empty());
            assert!(!model.display_name.is_empty());
            assert_eq!(model.effect_type, effect_type);
        }
    }
}

#[test]
fn supported_block_models_unsupported_type_errors() {
    let result = supported_block_models("nonexistent_type");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unsupported effect type"));
}

// --- supported_block_type tests ---

#[test]
fn supported_block_type_known_type_returns_some() {
    let entry = super::supported_block_type("preamp");
    assert!(entry.is_some());
    let entry = entry.unwrap();
    assert_eq!(entry.effect_type, "preamp");
    assert_eq!(entry.display_label, "PREAMP");
}

#[test]
fn supported_block_type_vst3_returns_some() {
    let entry = super::supported_block_type("vst3");
    assert!(entry.is_some());
    assert_eq!(entry.unwrap().display_label, "VST3");
}

#[test]
fn supported_block_type_unknown_returns_none() {
    assert!(super::supported_block_type("nonexistent").is_none());
}

// --- model_stream_kind tests ---

#[test]
fn model_stream_kind_non_utility_returns_empty() {
    assert_eq!(super::model_stream_kind("delay", "some_model"), "");
    assert_eq!(super::model_stream_kind("preamp", "american_clean"), "");
}

#[test]
#[ignore = "block-util crate is empty (utility blocks promoted to top-bar features in #320)"]
fn model_stream_kind_utility_returns_value() {
    let model = block_util::supported_models()[0];
    // Should not panic; may return empty or a stream kind string
    let _ = super::model_stream_kind("utility", model);
}

// --- model_knob_layout tests ---

#[test]
fn model_knob_layout_unknown_type_returns_empty() {
    let layout = super::model_knob_layout("nonexistent", "model");
    assert!(layout.is_empty());
}

#[test]
fn model_knob_layout_known_type_returns_slice() {
    let model = block_delay::supported_models()[0];
    // Should not panic; may return empty or populated slice
    let _ = super::model_knob_layout("delay", model);
}

// --- build_block_kind tests ---

#[test]
fn build_block_kind_valid_model_succeeds() {
    let model = block_reverb::supported_models()[0];
    let schema = crate::block::schema_for_block_model("reverb", model).unwrap();
    let params = crate::param::ParameterSet::default()
        .normalized_against(&schema)
        .unwrap();
    let kind = super::build_block_kind("reverb", model, params);
    assert!(kind.is_ok());
}

#[test]
fn build_block_kind_invalid_type_errors() {
    let result = super::build_block_kind(
        "nonexistent",
        "model",
        crate::param::ParameterSet::default(),
    );
    assert!(result.is_err());
}

// --- catalog model entries have supported_instruments ---

#[test]
fn catalog_model_entries_have_supported_instruments() {
    let models = supported_block_models("preamp").unwrap();
    for model in &models {
        assert!(
            !model.supported_instruments.is_empty(),
            "preamp model {} should have supported_instruments",
            model.model_id
        );
    }
}

// --- disk-backed packages also surface to GUI (issue #287) ---

#[test]
fn supported_block_models_includes_disk_package_for_block_type() {
    use plugin_loader::native_runtimes::NativeRuntime;
    use plugin_loader::manifest::BlockType;
    use crate::param::ParameterSet;
    use std::path::PathBuf;

    fn fake_schema() -> anyhow::Result<block_core::param::ModelParameterSchema> {
        Ok(block_core::param::ModelParameterSchema {
            effect_type: block_core::EFFECT_TYPE_PREAMP.into(),
            model: "test_disk_pkg_preamp".into(),
            display_name: "Test Disk Preamp".into(),
            audio_mode: block_core::ModelAudioMode::DualMono,
            parameters: Vec::new(),
        })
    }
    fn fake_validate(_: &ParameterSet) -> anyhow::Result<()> {
        Ok(())
    }
    fn fake_build(
        _: &ParameterSet,
        _: f32,
        _: block_core::AudioChannelLayout,
    ) -> anyhow::Result<block_core::BlockProcessor> {
        anyhow::bail!("not used in this test")
    }

    plugin_loader::registry::register_native_simple(
        "test_disk_pkg_preamp",
        "Test Disk Preamp",
        Some("test"),
        BlockType::Preamp,
        NativeRuntime {
            schema: fake_schema,
            validate: fake_validate,
            build: fake_build,
        },
    );
    // init() is OnceLock-backed — first caller wins. An earlier test
    // may have called it already, in which case our register_native
    // entry was queued before init and is still observable via
    // packages(). If init wasn't called yet, this call freezes the
    // registry now.
    plugin_loader::registry::init(&PathBuf::from("/nonexistent-test-path"));

    let models = supported_block_models("preamp").expect("preamp catalog");
    assert!(
        models.iter().any(|m| m.model_id == "test_disk_pkg_preamp"),
        "expected disk-package model 'test_disk_pkg_preamp' to surface in supported_block_models, \
         got: {:?}",
        models.iter().map(|m| m.model_id.as_str()).collect::<Vec<_>>()
    );
}
