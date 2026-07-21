//! Block model-ref / disk-package end-to-end tests (issue #792 split from
//! block_tests.rs). Shares delay_block via super::tests.

use crate::param::ParameterSet;
use domain::ids::BlockId;

use super::tests::delay_block;
use super::*;

#[test]
fn audio_descriptors_select_missing_selected_errors() {
    let model = block_delay::supported_models().first().unwrap();
    let opt = delay_block("sel::a", model);
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::missing".into()),
            options: vec![opt],
        }),
    };
    assert!(block.audio_descriptors().is_err());
}

// --- model_ref for all AudioBlockKind variants ---

#[test]
fn model_ref_nam_returns_some() {
    // model_ref does not validate params, so we can use default params
    let block = AudioBlock {
        id: BlockId("b:nam".into()),
        enabled: true,
        kind: AudioBlockKind::Nam(super::NamBlock {
            model: "neural_amp_modeler".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let mr = block.model_ref().expect("nam should expose model_ref");
    assert_eq!(mr.effect_type, "nam");
    assert_eq!(mr.model, "neural_amp_modeler");
}

#[test]
fn model_ref_core_returns_some() {
    let model = block_delay::supported_models().first().unwrap();
    let blk = delay_block("b:core", model);
    let mr = blk.model_ref().expect("core should expose model_ref");
    assert_eq!(mr.effect_type, "delay");
    assert_eq!(mr.model, *model);
}

#[test]
fn model_ref_select_returns_none() {
    let model = block_delay::supported_models().first().unwrap();
    let opt = delay_block("sel::a", model);
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::a".into()),
            options: vec![opt],
        }),
    };
    assert!(block.model_ref().is_none());
}

#[test]
fn model_ref_input_returns_none() {
    let block = AudioBlock {
        id: BlockId("b:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.model_ref().is_none());
}

#[test]
fn model_ref_output_returns_none() {
    let block = AudioBlock {
        id: BlockId("b:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.model_ref().is_none());
}

#[test]
fn model_ref_insert_returns_none() {
    let block = AudioBlock {
        id: BlockId("b:ins".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "fx".to_string(),
        }),
    };
    assert!(block.model_ref().is_none());
}

// --- parameter_descriptors for all AudioBlockKind variants ---

#[test]
fn parameter_descriptors_input_returns_empty() {
    let block = AudioBlock {
        id: BlockId("b:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.parameter_descriptors().unwrap().is_empty());
}

#[test]
fn parameter_descriptors_output_returns_empty() {
    let block = AudioBlock {
        id: BlockId("b:out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.parameter_descriptors().unwrap().is_empty());
}

#[test]
fn parameter_descriptors_core_returns_params() {
    let model = block_delay::supported_models().first().unwrap();
    let blk = delay_block("b:core", model);
    let descs = blk.parameter_descriptors().unwrap();
    assert!(
        !descs.is_empty(),
        "delay block should have parameter descriptors"
    );
}

#[test]
fn parameter_descriptors_select_delegates_to_selected() {
    let model = block_delay::supported_models().first().unwrap();
    let opt = delay_block("sel::a", model);
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::a".into()),
            options: vec![opt],
        }),
    };
    let descs = block.parameter_descriptors().unwrap();
    assert!(!descs.is_empty());
}

#[test]
fn parameter_descriptors_select_missing_selected_errors() {
    let model = block_delay::supported_models().first().unwrap();
    let opt = delay_block("sel::a", model);
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::missing".into()),
            options: vec![opt],
        }),
    };
    assert!(block.parameter_descriptors().is_err());
}

// --- SelectBlock validate_structure edge cases ---

#[test]
fn select_block_rejects_nested_select_option() {
    let model = block_delay::supported_models().first().unwrap();
    let inner_opt = delay_block("inner::a", model);
    let nested_select = AudioBlock {
        id: BlockId("sel::nested".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("inner::a".into()),
            options: vec![inner_opt],
        }),
    };
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::nested".into()),
            options: vec![nested_select],
        }),
    };
    let err = block
        .validate_params()
        .expect_err("nested select should fail");
    assert!(err.contains("select, input, output, or insert"));
}

#[test]
fn select_block_rejects_input_option() {
    let input_opt = AudioBlock {
        id: BlockId("sel::in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::in".into()),
            options: vec![input_opt],
        }),
    };
    let err = block
        .validate_params()
        .expect_err("input option should fail");
    assert!(err.contains("select, input, output, or insert"));
}

#[test]
fn select_block_rejects_output_option() {
    let output_opt = AudioBlock {
        id: BlockId("sel::out".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::out".into()),
            options: vec![output_opt],
        }),
    };
    let err = block
        .validate_params()
        .expect_err("output option should fail");
    assert!(err.contains("select, input, output, or insert"));
}

#[test]
fn select_block_rejects_insert_option() {
    let insert_opt = AudioBlock {
        id: BlockId("sel::ins".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "fx".to_string(),
        }),
    };
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::ins".into()),
            options: vec![insert_opt],
        }),
    };
    let err = block
        .validate_params()
        .expect_err("insert option should fail");
    assert!(err.contains("select, input, output, or insert"));
}

#[test]
fn select_block_exactly_eight_options_ok() {
    let model = block_delay::supported_models().first().unwrap();
    let options: Vec<_> = (0..8)
        .map(|i| delay_block(format!("sel::{i}"), model))
        .collect();
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::0".into()),
            options,
        }),
    };
    assert!(block.validate_params().is_ok());
}

// --- schema_for_block_model and normalize_block_params edge cases ---

#[test]
fn schema_for_unsupported_block_type_returns_error() {
    let result = schema_for_block_model("nonexistent_type", "some_model");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unsupported block type"));
}

#[test]
fn normalize_block_params_unsupported_type_returns_error() {
    let result = normalize_block_params("nonexistent_type", "some_model", ParameterSet::default());
    assert!(result.is_err());
}

#[test]
fn schema_covers_all_static_effect_types() {
    let types_and_models = [
        ("preamp", block_preamp::supported_models()),
        ("amp", block_amp::supported_models()),
        ("cab", block_cab::supported_models()),
        ("body", block_body::supported_models()),
        ("ir", block_ir::supported_models()),
        ("gain", block_gain::supported_models()),
        ("nam", block_nam::supported_models()),
        ("delay", block_delay::supported_models()),
        ("reverb", block_reverb::supported_models()),
        ("dynamics", block_dyn::supported_models()),
        ("filter", block_filter::supported_models()),
        ("wah", block_wah::supported_models()),
        ("pitch", block_pitch::supported_models()),
        ("modulation", block_mod::supported_models()),
    ];
    for (effect_type, models) in types_and_models {
        // Some block crates currently expose no native models — their
        // catalog lives in plugin-loader / disk packages instead. Skip
        // those rather than fail; the fallback path is exercised by
        // `disk_packages_load_through_full_block_pipeline` below.
        let Some(model) = models.first() else {
            continue;
        };
        let schema = schema_for_block_model(effect_type, model).unwrap();
        assert_eq!(schema.effect_type, effect_type);
    }
}

// --- build_audio_block_kind ---

#[test]
fn build_audio_block_kind_core_types() {
    let model = block_delay::supported_models().first().unwrap();
    let schema = schema_for_block_model("delay", model).unwrap();
    let params = ParameterSet::default().normalized_against(&schema).unwrap();
    let kind = super::build_audio_block_kind("delay", model, params).unwrap();
    assert!(matches!(kind, AudioBlockKind::Core(_)));
}

#[test]
fn build_audio_block_kind_nam_type() {
    // build_audio_block_kind just constructs the enum variant without
    // validating params, so we can use empty params here.
    let kind = super::build_audio_block_kind("nam", "neural_amp_modeler", ParameterSet::default())
        .unwrap();
    assert!(matches!(kind, AudioBlockKind::Nam(_)));
}

#[test]
fn build_audio_block_kind_unsupported_type_errors() {
    let result = super::build_audio_block_kind("nonexistent", "model", ParameterSet::default());
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unsupported block type"));
}

// ── disk-package end-to-end: schema + normalize + audio_descriptors all
// must accept models that live only in plugin_loader::registry, not in
// the legacy block-* registries. Single test combines coverage because
// plugin_loader::registry is OnceLock-backed (init freezes it once).
// Issue #287.
#[test]
fn disk_packages_load_through_full_block_pipeline() {
    use crate::param::{ModelParameterSchema, ParameterSet};
    use plugin_loader::manifest::BlockType;
    use plugin_loader::native_runtimes::NativeRuntime;
    use std::path::PathBuf;

    fn empty_schema(
        effect_type: &'static str,
        model_id: &'static str,
    ) -> impl Fn() -> anyhow::Result<ModelParameterSchema> {
        move || {
            Ok(ModelParameterSchema {
                effect_type: effect_type.into(),
                model: model_id.into(),
                display_name: model_id.into(),
                audio_mode: block_core::ModelAudioMode::DualMono,
                parameters: Vec::new(),
            })
        }
    }
    fn ok_validate(_: &ParameterSet) -> anyhow::Result<()> {
        Ok(())
    }
    fn err_build(
        _: &ParameterSet,
        _: f32,
        _: block_core::AudioChannelLayout,
    ) -> anyhow::Result<block_core::BlockProcessor> {
        anyhow::bail!("not exercised in this test")
    }

    fn bare_schema() -> anyhow::Result<ModelParameterSchema> {
        Ok(ModelParameterSchema {
            effect_type: "".into(),
            model: "".into(),
            display_name: "".into(),
            audio_mode: block_core::ModelAudioMode::DualMono,
            parameters: Vec::new(),
        })
    }

    // One fake disk package per BlockType the catalog cares about. All
    // share the same noop NativeRuntime — these tests don't exercise
    // build, only the load/normalize/schema pipeline.
    let fixtures: &[(&str, BlockType, &str)] = &[
        ("preamp", BlockType::Preamp, "test_disk_preamp_e2e"),
        ("amp", BlockType::Amp, "test_disk_amp_e2e"),
        ("cab", BlockType::Cab, "test_disk_cab_e2e"),
        ("body", BlockType::Body, "test_disk_body_e2e"),
        ("gain", BlockType::GainPedal, "test_disk_gain_e2e"),
        ("delay", BlockType::Delay, "test_disk_delay_e2e"),
        ("reverb", BlockType::Reverb, "test_disk_reverb_e2e"),
        ("modulation", BlockType::Mod, "test_disk_mod_e2e"),
        ("dynamics", BlockType::Dyn, "test_disk_dyn_e2e"),
        ("filter", BlockType::Filter, "test_disk_filter_e2e"),
        ("pitch", BlockType::Pitch, "test_disk_pitch_e2e"),
        ("wah", BlockType::Wah, "test_disk_wah_e2e"),
    ];
    for (_, block_type, id) in fixtures {
        plugin_loader::registry::register_native_simple(
            id,
            id,
            Some("test"),
            *block_type,
            NativeRuntime {
                schema: bare_schema,
                validate: ok_validate,
                build: err_build,
            },
        );
    }
    plugin_loader::registry::init(&PathBuf::from("/nonexistent-test-path"));

    // schema_for_block_model fallback resolves each disk package.
    for (effect_type, _, id) in fixtures {
        let schema = crate::block::schema_for_block_model(effect_type, id);
        assert!(
            schema.is_ok(),
            "schema_for_block_model({effect_type}, {id}) should fall through to plugin_loader::registry, got: {:?}",
            schema.err()
        );
    }

    // normalize_block_params accepts each disk package even though the
    // legacy validate_*_params returns Err for unknown ids.
    for (effect_type, _, id) in fixtures {
        let result = crate::block::normalize_block_params(effect_type, id, ParameterSet::default());
        assert!(
            result.is_ok(),
            "normalize_block_params({effect_type}, {id}) should accept disk package, got: {:?}",
            result.err()
        );
    }

    // catalog::supported_block_models surfaces every disk package id.
    for (effect_type, _, id) in fixtures {
        let models = crate::catalog::supported_block_models(effect_type)
            .unwrap_or_else(|err| panic!("supported_block_models({effect_type}) failed: {err}"));
        assert!(
            models.iter().any(|m| m.model_id == *id),
            "supported_block_models({effect_type}) should include disk package {id}, got: {:?}",
            models
                .iter()
                .map(|m| m.model_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    // Suppress dead-code warnings: helper exists for future param-driven tests.
    let _ = empty_schema;
}
