//! Tests for `project::block`. Lifted out of `block.rs` so the production
//! file stays under the size cap. Re-attached as `mod tests` of the parent
//! via `#[cfg(test)] #[path = "block_tests.rs"] mod tests;`, so every
//! `super::*` reference resolves unchanged.

use super::{
    normalize_block_params, schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock,
    InputBlock, InsertBlock, OutputBlock, SelectBlock,
};
use crate::param::ParameterSet;
use domain::ids::BlockId;

#[test]
fn project_contract_exposes_family_schemas() {
    let families = [
        ("preamp", block_preamp::supported_models()),
        ("amp", block_amp::supported_models()),
        ("cab", block_cab::supported_models()),
        ("ir", block_ir::supported_models()),
        ("wah", block_wah::supported_models()),
        ("delay", block_delay::supported_models()),
    ];

    for (effect_type, models) in families {
        for model in models {
            let schema = schema_for_block_model(effect_type, model).expect("schema should exist");
            assert_eq!(schema.model, *model);
            assert_eq!(
                schema.effect_type, effect_type,
                "schema for {effect_type}:{model} should expose matching family"
            );
            assert!(
                !schema.parameters.is_empty(),
                "schema for {effect_type}:{model} should expose parameters"
            );
        }
    }
}

#[test]
fn project_contract_normalizes_defaults_for_supported_families() {
    let families = [
        ("preamp", block_preamp::supported_models()),
        ("amp", block_amp::supported_models()),
        ("cab", block_cab::supported_models()),
        ("ir", block_ir::supported_models()),
        ("wah", block_wah::supported_models()),
        ("delay", block_delay::supported_models()),
    ];

    for (effect_type, models) in families {
        for model in models {
            let schema = schema_for_block_model(effect_type, model).expect("schema should exist");
            let normalized = normalize_block_params(effect_type, model, ParameterSet::default());
            let has_complete_defaults = schema
                .parameters
                .iter()
                .all(|parameter| parameter.default_value.is_some());

            if has_complete_defaults {
                let normalized = normalized.expect("params should normalize with schema defaults");
                assert_eq!(normalized.values.len(), schema.parameters.len());
            } else {
                assert!(
                    normalized.is_err(),
                    "model {effect_type}:{model} should reject empty params when schema has required fields without defaults"
                );
            }
        }
    }
}

#[test]
fn select_block_requires_at_least_one_option() {
    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("chain:0:block:0::missing".into()),
            options: Vec::new(),
        }),
    };

    let error = block
        .validate_params()
        .expect_err("empty select options should fail");

    assert!(error.contains("at least one option"));
}

#[test]
fn select_block_rejects_missing_selected_option() {
    let first_model = block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model");

    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("chain:0:block:0::missing".into()),
            options: vec![delay_block("chain:0:block:0::a", first_model)],
        }),
    };

    let error = block
        .validate_params()
        .expect_err("select without selected option should fail");

    assert!(error.contains("selected option"));
}

#[test]
fn select_block_rejects_mixed_effect_types() {
    let delay_model = block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model");
    let reverb_model = block_reverb::supported_models()
        .first()
        .expect("block-reverb must expose at least one model");

    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("chain:0:block:0::delay".into()),
            options: vec![
                delay_block("chain:0:block:0::delay", delay_model),
                reverb_block("chain:0:block:0::reverb", reverb_model),
            ],
        }),
    };

    let error = block
        .validate_params()
        .expect_err("mixed select families should fail");

    assert!(error.contains("same effect type"));
}

#[test]
fn select_block_rejects_more_than_eight_options() {
    let model = block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model");
    let options = (0..9)
        .map(|index| delay_block(format!("chain:0:block:0::{index}"), model))
        .collect::<Vec<_>>();

    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("chain:0:block:0::0".into()),
            options,
        }),
    };

    let error = block
        .validate_params()
        .expect_err("select with more than eight options should fail");

    assert!(error.contains("up to 8 options"));
}

pub(super) fn delay_block(id: impl Into<String>, model: &str) -> AudioBlock {
    let schema = schema_for_block_model("delay", model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults should normalize");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn reverb_block(id: impl Into<String>, model: &str) -> AudioBlock {
    let schema = schema_for_block_model("reverb", model).expect("reverb schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("reverb defaults should normalize");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "reverb".to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

// --- InsertBlock tests ---

#[test]
fn insert_block_clone_equality() {
    let insert = InsertBlock {
        model: "standard".to_string(),
        io: "mk300".to_string(),
    };
    let block = AudioBlock {
        id: BlockId("chain:0:insert:0".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(insert.clone()),
    };
    let cloned = block.clone();
    assert_eq!(block, cloned);
    assert!(matches!(&block.kind, AudioBlockKind::Insert(ib) if ib.io == "mk300"));
}

#[test]
fn insert_block_in_chain_structure() {
    let chain = crate::chain::Chain {
        id: domain::ids::ChainId("chain:0".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![
            AudioBlock {
                id: BlockId("chain:0:input:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    io: String::new(),
                    endpoint: String::new(),
                }),
            },
            AudioBlock {
                id: BlockId("chain:0:insert:0".into()),
                enabled: true,
                kind: AudioBlockKind::Insert(InsertBlock {
                    model: "standard".to_string(),
                    io: "mk300".to_string(),
                }),
            },
            AudioBlock {
                id: BlockId("chain:0:output:0".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    io: String::new(),
                    endpoint: String::new(),
                }),
            },
        ],
        di_output: None,
    };
    let inserts = chain.insert_blocks();
    assert_eq!(inserts.len(), 1);
    assert_eq!(inserts[0].0, 1); // index 1
    assert_eq!(inserts[0].1.io, "mk300");
}

#[test]
fn disabled_insert_block_validates_ok() {
    let block = AudioBlock {
        id: BlockId("chain:0:insert:0".into()),
        enabled: false,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: String::new(),
        }),
    };
    assert!(block.validate_params().is_ok());
    assert_eq!(block.parameter_descriptors().unwrap(), Vec::new());
    assert_eq!(block.audio_descriptors().unwrap(), Vec::new());
    assert!(block.model_ref().is_none());
}

// --- validate_params for all AudioBlockKind variants ---

#[test]
fn validate_params_input_block_always_ok() {
    let block = AudioBlock {
        id: BlockId("b:input".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.validate_params().is_ok());
}

#[test]
fn validate_params_output_block_always_ok() {
    let block = AudioBlock {
        id: BlockId("b:output".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.validate_params().is_ok());
}

#[test]
fn validate_params_enabled_insert_block_always_ok() {
    let block = AudioBlock {
        id: BlockId("b:insert".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "fx".to_string(),
        }),
    };
    assert!(block.validate_params().is_ok());
}

#[test]
fn validate_params_disabled_block_skips_validation() {
    // A disabled Core block with an invalid model should still pass
    let block = AudioBlock {
        id: BlockId("b:disabled".into()),
        enabled: false,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: "nonexistent_model".to_string(),
            params: ParameterSet::default(),
        }),
    };
    assert!(block.validate_params().is_ok());
}

#[test]
fn validate_params_core_block_valid_model() {
    let model = block_delay::supported_models()
        .first()
        .expect("at least one delay model");
    let schema = schema_for_block_model("delay", model).unwrap();
    let params = ParameterSet::default().normalized_against(&schema).unwrap();
    let block = AudioBlock {
        id: BlockId("b:core".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".to_string(),
            model: model.to_string(),
            params,
        }),
    };
    assert!(block.validate_params().is_ok());
}

#[test]
fn validate_params_nam_block_disabled_ok() {
    // The generic NAM model requires model_path (no default), so we test
    // disabled NAM blocks which skip validation entirely.
    let block = AudioBlock {
        id: BlockId("b:nam".into()),
        enabled: false,
        kind: AudioBlockKind::Nam(super::NamBlock {
            model: "neural_amp_modeler".to_string(),
            params: ParameterSet::default(),
        }),
    };
    assert!(block.validate_params().is_ok());
}

#[test]
fn validate_params_select_block_valid() {
    let model = block_delay::supported_models()
        .first()
        .expect("at least one delay model");
    let opt = delay_block("sel::a", model);
    let block = AudioBlock {
        id: BlockId("b:sel".into()),
        enabled: true,
        kind: AudioBlockKind::Select(SelectBlock {
            selected_block_id: BlockId("sel::a".into()),
            options: vec![opt],
        }),
    };
    assert!(block.validate_params().is_ok());
}

// --- audio_descriptors for all AudioBlockKind variants ---

#[test]
fn audio_descriptors_input_returns_empty() {
    let block = AudioBlock {
        id: BlockId("b:input".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.audio_descriptors().unwrap().is_empty());
}

#[test]
fn audio_descriptors_output_returns_empty() {
    let block = AudioBlock {
        id: BlockId("b:output".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            io: String::new(),
            endpoint: String::new(),
        }),
    };
    assert!(block.audio_descriptors().unwrap().is_empty());
}

#[test]
fn audio_descriptors_insert_returns_empty() {
    let block = AudioBlock {
        id: BlockId("b:insert".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "fx".to_string(),
        }),
    };
    assert!(block.audio_descriptors().unwrap().is_empty());
}

#[test]
fn audio_descriptors_disabled_block_returns_empty() {
    let model = block_delay::supported_models().first().unwrap();
    let mut blk = delay_block("b:dis", model);
    blk.enabled = false;
    assert!(blk.audio_descriptors().unwrap().is_empty());
}

#[test]
fn audio_descriptors_core_block_returns_one() {
    let model = block_reverb::supported_models().first().unwrap();
    let blk = reverb_block("b:core", model);
    let descs = blk.audio_descriptors().unwrap();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].effect_type, "reverb");
    assert_eq!(descs[0].block_id.0, "b:core");
}

#[test]
fn audio_descriptors_disabled_nam_block_returns_empty() {
    // Generic NAM requires model_path; test disabled path which returns empty
    let block = AudioBlock {
        id: BlockId("b:nam".into()),
        enabled: false,
        kind: AudioBlockKind::Nam(super::NamBlock {
            model: "neural_amp_modeler".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let descs = block.audio_descriptors().unwrap();
    assert!(descs.is_empty());
}

#[test]
fn audio_descriptors_select_delegates_to_selected() {
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
    let descs = block.audio_descriptors().unwrap();
    assert_eq!(descs.len(), 1);
    assert_eq!(descs[0].effect_type, "delay");
}
