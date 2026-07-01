//! Tests for `infra-yaml`. Lifted out of `lib.rs` so the production file
//! stays under the size cap. Re-attached as `mod tests` of the parent via
//! `#[cfg(test)] #[path = "lib_tests.rs"] mod tests;` — content kept at
//! the original 4-space indent so raw-string YAML literals stay intact.

use super::{
    load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset, YamlProjectRepository,
};
use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock, SelectBlock,
};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

#[test]
fn save_project_creates_yaml_that_roundtrips_basic_project() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("project.yaml");
    let repository = YamlProjectRepository {
        path: project_path.clone(),
    };
    let original = Project {
        name: Some("Test Project".into()),
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("Guitar 1".into()),
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
                        io: "main".to_string(),
                        endpoint: "Guitar In".to_string(),
                    }),
                },
                AudioBlock {
                    id: BlockId("chain:0:output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        io: "main".to_string(),
                        endpoint: "Monitor Out".to_string(),
                    }),
                },
            ],
            di_output: None,
        }],
        midi: None,
    };

    repository
        .save_project(&original)
        .expect("project save should succeed");

    assert!(project_path.exists(), "project yaml should be written");

    let loaded = repository
        .load_current_project()
        .expect("saved project should load");

    assert_eq!(loaded.name, original.name);
    assert_eq!(loaded.chains.len(), 1);
    assert_eq!(loaded.chains[0].description, original.chains[0].description);
    let loaded_inputs = loaded.chains[0].input_blocks();
    assert_eq!(loaded_inputs.len(), 1);
    assert_eq!(loaded_inputs[0].1.io, "main");
    assert_eq!(loaded_inputs[0].1.endpoint, "Guitar In");
    let loaded_outputs = loaded.chains[0].output_blocks();
    assert_eq!(loaded_outputs.len(), 1);
    assert_eq!(loaded_outputs[0].1.io, "main");
    assert_eq!(loaded_outputs[0].1.endpoint, "Monitor Out");
}

#[test]
fn load_project_ignores_removed_or_invalid_blocks() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("project.yaml");
    let valid_delay_model = block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model");
    fs::write(
        &project_path,
        format!(
            r#"
chains:
  - enabled: true
    blocks:
      - type: core_nam
        enabled: true
        model_id: legacy
      - type: delay
        enabled: true
        model: {valid_delay_model}
        params:
          time_ms: 200
          feedback: 50
          mix: 30
"#,
        ),
    )
    .expect("project yaml should be written");

    let repository = YamlProjectRepository { path: project_path };
    let project = repository
        .load_current_project()
        .expect("project should load while skipping invalid blocks");

    assert_eq!(project.chains.len(), 1);
    // The invalid core_nam block is dropped; only the valid delay survives.
    let audio_blocks: Vec<_> = project.chains[0]
        .blocks
        .iter()
        .filter(|b| {
            !matches!(
                &b.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
            )
        })
        .collect();
    assert_eq!(audio_blocks.len(), 1);
    assert_eq!(
        audio_blocks[0]
            .model_ref()
            .expect("remaining block should expose model")
            .model,
        *valid_delay_model
    );
}

#[test]
fn load_preset_ignores_unknown_models() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let preset_path: PathBuf = temp_dir.path().join("example.yaml");
    let valid_delay_model = block_delay::supported_models()
        .first()
        .expect("block-delay must expose at least one model");
    fs::write(
        &preset_path,
        format!(
            r#"
id: example
blocks:
  - type: delay
    model: deleted_model
    params:
      time_ms: 200
      feedback: 50
      mix: 30
  - type: delay
    model: {valid_delay_model}
    params:
      time_ms: 210
      feedback: 40
      mix: 25
"#,
        ),
    )
    .expect("preset yaml should be written");

    let preset = load_chain_preset_file(&preset_path)
        .expect("preset should load while skipping invalid blocks");

    assert_eq!(preset.blocks.len(), 1);
    assert_eq!(
        preset.blocks[0]
            .model_ref()
            .expect("remaining block should expose model")
            .model,
        *valid_delay_model
    );
}

#[test]
fn load_project_supports_generic_select_options() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("project.yaml");
    let delay_models = block_delay::supported_models();
    let first_model = delay_models
        .first()
        .expect("block-delay must expose at least one model");
    let second_model = delay_models.get(1).unwrap_or(first_model);

    fs::write(
        &project_path,
        format!(
            r#"
chains:
  - enabled: true
    input_device_id: input-device
    input_channels: [0]
    output_device_id: output-device
    output_channels: [0]
    blocks:
      - type: select
        enabled: true
        selected: delay_b
        options:
          - id: delay_a
            type: delay
            model: {first_model}
            params:
              time_ms: 120
              feedback: 20
              mix: 30
          - id: delay_b
            type: delay
            model: {second_model}
            params:
              time_ms: 240
              feedback: 40
              mix: 25
"#,
        ),
    )
    .expect("project yaml should be written");

    let repository = YamlProjectRepository { path: project_path };
    let project = repository
        .load_current_project()
        .expect("project should load generic select blocks");

    // Find the first non-I/O block (should be the select block)
    let audio_block = project.chains[0]
        .blocks
        .iter()
        .find(|b| {
            !matches!(
                &b.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
            )
        })
        .expect("should have at least one audio block");
    let select = match &audio_block.kind {
        AudioBlockKind::Select(select) => select,
        other => panic!("expected select block, got {:?}", other),
    };
    assert_eq!(select.options.len(), 2);
    assert_eq!(select.selected_block_id.0, "chain:0:block:0::delay_b");
}

#[test]
fn preset_roundtrips_generic_select_options() {
    let temp_dir = tempdir().expect("temp dir should be created");
    let preset_path: PathBuf = temp_dir.path().join("select.yaml");
    let delay_models = block_delay::supported_models();
    let first_model = delay_models
        .first()
        .expect("block-delay must expose at least one model");
    let second_model = delay_models.get(1).unwrap_or(first_model);
    let preset = ChainBlocksPreset {
        id: "select".into(),
        name: Some("Delay Select".into()),
        volume: 100.0,
        instrument: "electric_guitar".to_string(),
        blocks: vec![AudioBlock {
            id: BlockId("preset:select:block:0".into()),
            enabled: true,
            kind: AudioBlockKind::Select(SelectBlock {
                selected_block_id: BlockId("preset:select:block:0::delay_b".into()),
                options: vec![
                    delay_block("preset:select:block:0::delay_a", first_model, 120.0),
                    delay_block("preset:select:block:0::delay_b", second_model, 240.0),
                ],
            }),
        }],
    };

    save_chain_preset_file(&preset_path, &preset).expect("preset save should succeed");
    let raw = fs::read_to_string(&preset_path).expect("saved preset should be readable");
    assert!(raw.contains("type: select"));
    assert!(raw.contains("- id: delay_a"));
    assert!(raw.contains("- id: delay_b"));

    let loaded = load_chain_preset_file(&preset_path).expect("preset should reload");
    let select = match &loaded.blocks[0].kind {
        AudioBlockKind::Select(select) => select,
        other => panic!("expected select block, got {:?}", other),
    };
    assert_eq!(select.selected_block_id.0, "preset:select:block:0::delay_b");
    assert_eq!(select.options.len(), 2);
}

fn delay_block(id: impl Into<String>, model: &str, time_ms: f32) -> AudioBlock {
    let schema =
        project::block::schema_for_block_model("delay", model).expect("delay schema exists");
    let mut params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults should normalize");
    params.insert(
        "time_ms",
        domain::value_objects::ParameterValue::Float(time_ms),
    );
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

#[test]
fn insert_block_yaml_roundtrip() {
    use project::block::InsertBlock;
    let block = AudioBlock {
        id: BlockId("chain:0:block:1".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "mk300".to_string(),
        }),
    };
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 1).expect("into block");
    assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.io == "mk300"));
    assert!(matches!(&restored.kind, AudioBlockKind::Insert(ib) if ib.model == "standard"));
}

#[test]
fn disabled_insert_block_yaml_roundtrip() {
    use project::block::InsertBlock;
    let block = AudioBlock {
        id: BlockId("chain:0:block:2".into()),
        enabled: false,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: String::new(),
        }),
    };
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 2).expect("into block");
    assert!(!restored.enabled);
    assert!(matches!(&restored.kind, AudioBlockKind::Insert(_)));
}

// ─── Guard: Insert block YAML carries the `io` binding, not raw send/return (#716 model A) ───

/// Regression guard for issue #716 (model A).
///
/// An insert now references ONE I/O binding (`io`) instead of embedding raw
/// send/return device endpoints. The send resolves to that binding's output
/// and the return to its input via the per-machine registry. The serialized
/// YAML must therefore carry `io` and must NOT carry the deleted
/// `send:`/`return:` device blocks.
///
/// If this test fails, the legacy embedded-endpoint model leaked back into
/// Insert serialization — restore the `{ model, io }` shape.
#[test]
fn insert_block_yaml_carries_io_binding_not_raw_endpoints() {
    use project::block::InsertBlock;
    let block = AudioBlock {
        id: BlockId("chain:0:block:1".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".to_string(),
            io: "fx-loop".to_string(),
        }),
    };
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize to value");
    let yaml_str = serde_yaml::to_string(&value).expect("serialize to string");

    // Guard: model A — the insert references its binding via `io`.
    assert!(
        yaml_str.contains("io: fx-loop"),
        "Insert block YAML must carry the 'io' binding id (#716 model A): {yaml_str}"
    );
    // Guard: the deleted raw send/return device model must NOT come back.
    assert!(
        !yaml_str.contains("send:"),
        "Insert block YAML must NOT contain a raw 'send:' endpoint (#716 model A): {yaml_str}"
    );
    assert!(
        !yaml_str.contains("return:"),
        "Insert block YAML must NOT contain a raw 'return:' endpoint (#716 model A): {yaml_str}"
    );
}

// ─── Helper: build a CoreBlock AudioBlock for a given effect type + model ───

fn core_block(
    id: &str,
    effect_type: &str,
    model: &str,
    param_overrides: Vec<(&str, domain::value_objects::ParameterValue)>,
) -> AudioBlock {
    let schema =
        project::block::schema_for_block_model(effect_type, model).expect("schema should exist");
    let mut params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("defaults should normalize");
    for (k, v) in param_overrides {
        params.insert(k, v);
    }
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: effect_type.to_string(),
            model: model.to_string(),
            params,
        }),
    }
}

fn first_model<'a>(models: &'a [&'a str]) -> &'a str {
    models
        .first()
        .expect("block crate must expose at least one model")
}

/// Same as `first_model` but returns `None` when the block crate exposes
/// no native models. Used by roundtrip tests for block crates whose only
/// models live in plugin-loader / disk packages — `block-body`, `block-ir`,
/// `block-pitch`, `block-nam` — so the test no-ops instead of panicking.
fn first_model_optional<'a>(models: &'a [&'a str]) -> Option<&'a str> {
    models.first().copied()
}

fn assert_core_roundtrip(effect_type: &str, model: &str) {
    let block = core_block("chain:0:block:0", effect_type, model, Vec::new());
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
    match &restored.kind {
        AudioBlockKind::Core(core) => {
            assert_eq!(core.effect_type, effect_type);
            assert_eq!(core.model, model);
        }
        other => panic!(
            "expected Core block for effect_type={}, got {:?}",
            effect_type, other
        ),
    }
}

// ─── Roundtrip tests for all core block types ───

#[test]
fn roundtrip_preamp_block_preserves_type_and_model() {
    assert_core_roundtrip("preamp", first_model(block_preamp::supported_models()));
}

#[test]
fn roundtrip_amp_block_preserves_type_and_model() {
    assert_core_roundtrip("amp", first_model(block_amp::supported_models()));
}

#[test]
fn roundtrip_cab_block_preserves_type_and_model() {
    assert_core_roundtrip("cab", first_model(block_cab::supported_models()));
}

#[test]
fn roundtrip_body_block_preserves_type_and_model() {
    let Some(model) = first_model_optional(block_body::supported_models()) else {
        return;
    };
    assert_core_roundtrip("body", model);
}

#[test]
fn roundtrip_gain_block_preserves_type_and_model() {
    assert_core_roundtrip("gain", first_model(block_gain::supported_models()));
}

#[test]
fn roundtrip_delay_block_preserves_type_and_model() {
    assert_core_roundtrip("delay", first_model(block_delay::supported_models()));
}

#[test]
fn roundtrip_reverb_block_preserves_type_and_model() {
    assert_core_roundtrip("reverb", first_model(block_reverb::supported_models()));
}

#[test]
fn roundtrip_dynamics_block_preserves_type_and_model() {
    assert_core_roundtrip("dynamics", first_model(block_dyn::supported_models()));
}

#[test]
fn roundtrip_filter_block_preserves_type_and_model() {
    assert_core_roundtrip("filter", first_model(block_filter::supported_models()));
}

#[test]
fn roundtrip_wah_block_preserves_type_and_model() {
    assert_core_roundtrip("wah", first_model(block_wah::supported_models()));
}

#[test]
fn roundtrip_modulation_block_preserves_type_and_model() {
    assert_core_roundtrip("modulation", first_model(block_mod::supported_models()));
}

#[test]
fn roundtrip_pitch_block_preserves_type_and_model() {
    let Some(model) = first_model_optional(block_pitch::supported_models()) else {
        return;
    };
    assert_core_roundtrip("pitch", model);
}

#[test]
fn roundtrip_ir_block_serializes_and_deserializes_yaml() {
    use domain::value_objects::ParameterValue;
    // IR normalization validates the file exists on disk, so we only test
    // the YAML serialization layer (from_audio_block -> to_value -> back).
    let Some(model) = first_model_optional(block_ir::supported_models()) else {
        return;
    };
    let mut params = ParameterSet::default();
    params.insert("file", ParameterValue::String("/some/path.wav".into()));
    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "ir".to_string(),
            model: model.to_string(),
            params,
        }),
    };
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    // Verify the serialized YAML has the correct type and model
    let yaml_str = serde_yaml::to_string(&value).expect("to string");
    assert!(yaml_str.contains("type: ir"));
    assert!(yaml_str.contains(&format!("model: {}", model)));
    assert!(yaml_str.contains("/some/path.wav"));
}

#[test]
fn roundtrip_full_rig_block_preserves_type_and_model() {
    let models = block_full_rig::supported_models();
    if models.is_empty() {
        // full_rig has no models yet (reserved for future use), skip
        return;
    }
    assert_core_roundtrip("full_rig", first_model(models));
}

// ─── Empty project ───

#[test]
fn serialize_empty_project_roundtrips() {
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: Vec::new(),
        midi: None,
    };
    let yaml_str = super::serialize_project(&project).expect("serialize should succeed");
    let dto: super::ProjectYaml = serde_yaml::from_str(&yaml_str).expect("should parse back");
    let loaded = dto.into_project().expect("should convert");
    assert!(loaded.name.is_none());
    assert!(loaded.chains.is_empty());
    assert!(loaded.device_settings.is_empty());
}

// ─── Chain with only input + output (no effect blocks) ───

#[test]
fn chain_with_only_io_blocks_roundtrips() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("io_only.yaml");
    let repo = YamlProjectRepository { path: path.clone() };
    let project = Project {
        name: Some("IO Only".into()),
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("Empty chain".into()),
            instrument: "electric_guitar".to_string(),
            enabled: false,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![
                AudioBlock {
                    id: BlockId("chain:0:input:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Input(InputBlock {
                        model: "standard".to_string(),
                        io: "main".to_string(),
                        endpoint: "In 1".to_string(),
                    }),
                },
                AudioBlock {
                    id: BlockId("chain:0:output:0".into()),
                    enabled: true,
                    kind: AudioBlockKind::Output(OutputBlock {
                        model: "standard".to_string(),
                        io: "main".to_string(),
                        endpoint: "Out 1".to_string(),
                    }),
                },
            ],
            di_output: None,
        }],
        midi: None,
    };
    repo.save_project(&project).expect("save");
    let loaded = repo.load_current_project().expect("load");
    assert_eq!(loaded.chains[0].blocks.len(), 2);
    assert!(matches!(
        &loaded.chains[0].blocks[0].kind,
        AudioBlockKind::Input(_)
    ));
    assert!(matches!(
        &loaded.chains[0].blocks[1].kind,
        AudioBlockKind::Output(_)
    ));
    // No effect blocks
    let effect_blocks: Vec<_> = loaded.chains[0]
        .blocks
        .iter()
        .filter(|b| {
            !matches!(
                &b.kind,
                AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
            )
        })
        .collect();
    assert!(effect_blocks.is_empty());
}

// ─── Parameter boundary values ───

#[test]
fn parameter_boundary_zero_value_roundtrips() {
    use domain::value_objects::ParameterValue;
    // `mix` is 0..=1 on every delay model, so 0.0 is always in range; the
    // earlier `time_ms=0.0` worked accidentally on some models but several
    // newer ones (e.g. `analog_warm`) require time_ms >= 1.
    let block = core_block(
        "chain:0:block:0",
        "delay",
        first_model(block_delay::supported_models()),
        vec![("mix", ParameterValue::Float(0.0))],
    );
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
    if let AudioBlockKind::Core(core) = &restored.kind {
        let mix = core.params.get("mix");
        assert!(mix.is_some(), "mix should be present");
        match mix.unwrap() {
            domain::value_objects::ParameterValue::Float(v) => assert_eq!(*v, 0.0),
            domain::value_objects::ParameterValue::Int(v) => assert_eq!(*v, 0),
            other => panic!("unexpected type for mix: {:?}", other),
        }
    } else {
        panic!("expected Core block");
    }
}

#[test]
fn parameter_boundary_max_value_roundtrips() {
    use domain::value_objects::ParameterValue;
    let block = core_block(
        "chain:0:block:0",
        "delay",
        first_model(block_delay::supported_models()),
        vec![("mix", ParameterValue::Float(100.0))],
    );
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
    if let AudioBlockKind::Core(core) = &restored.kind {
        let mix = core.params.get("mix");
        assert!(mix.is_some());
        match mix.unwrap() {
            domain::value_objects::ParameterValue::Float(v) => assert_eq!(*v, 100.0),
            other => panic!("unexpected type for mix: {:?}", other),
        }
    } else {
        panic!("expected Core block");
    }
}

// ─── flatten_parameter_set edge cases ───

#[test]
fn flatten_parameter_set_null_returns_empty() {
    let result = super::flatten_parameter_set(serde_yaml::Value::Null)
        .expect("null should flatten to empty");
    assert!(result.values.is_empty());
}

#[test]
fn flatten_parameter_set_nested_mapping_flattens_with_dot_notation() {
    use serde_yaml::Value;
    let yaml: Value = serde_yaml::from_str(
        r#"
eq:
  low: 50.0
  high: 80.0
volume: 75.0
"#,
    )
    .expect("parse");
    let result = super::flatten_parameter_set(yaml).expect("flatten");
    assert!(result.values.contains_key("eq.low"));
    assert!(result.values.contains_key("eq.high"));
    assert!(result.values.contains_key("volume"));
}

#[test]
fn flatten_parameter_set_non_mapping_returns_error() {
    use serde_yaml::Value;
    let yaml = Value::String("not a mapping".into());
    let result = super::flatten_parameter_set(yaml);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("params must be a mapping"));
}

#[test]
fn flatten_parameter_set_bool_and_string_values() {
    use serde_yaml::Value;
    let yaml: Value = serde_yaml::from_str(
        r#"
mute: true
mode: clean
"#,
    )
    .expect("parse");
    let result = super::flatten_parameter_set(yaml).expect("flatten");
    assert_eq!(
        result.values.get("mute"),
        Some(&domain::value_objects::ParameterValue::Bool(true))
    );
    assert_eq!(
        result.values.get("mode"),
        Some(&domain::value_objects::ParameterValue::String(
            "clean".into()
        ))
    );
}

// ─── parameter_set_to_yaml_value edge cases ───

#[test]
fn parameter_set_to_yaml_value_empty_returns_empty_mapping() {
    let params = ParameterSet::default();
    let value = super::parameter_set_to_yaml_value(&params);
    match value {
        serde_yaml::Value::Mapping(m) => assert!(m.is_empty()),
        other => panic!("expected empty mapping, got {:?}", other),
    }
}

#[test]
fn parameter_set_to_yaml_value_nested_keys_produce_nested_mapping() {
    use domain::value_objects::ParameterValue;
    let mut params = ParameterSet::default();
    params.insert("eq.low", ParameterValue::Float(30.0));
    params.insert("eq.high", ParameterValue::Float(70.0));
    params.insert("volume", ParameterValue::Float(50.0));

    let value = super::parameter_set_to_yaml_value(&params);
    let yaml_str = serde_yaml::to_string(&value).expect("serialize");
    assert!(yaml_str.contains("eq:"));
    assert!(yaml_str.contains("low:"));
    assert!(yaml_str.contains("high:"));
    assert!(yaml_str.contains("volume:"));
}

#[test]
fn parameter_set_to_yaml_value_null_bool_int_string() {
    use domain::value_objects::ParameterValue;
    let mut params = ParameterSet::default();
    params.insert("a_null", ParameterValue::Null);
    params.insert("a_bool", ParameterValue::Bool(false));
    params.insert("an_int", ParameterValue::Int(42));
    params.insert("a_str", ParameterValue::String("hello".into()));

    let value = super::parameter_set_to_yaml_value(&params);
    // Roundtrip back
    let restored = super::flatten_parameter_set(value).expect("flatten roundtrip");
    assert_eq!(restored.values.get("a_null"), Some(&ParameterValue::Null));
    assert_eq!(
        restored.values.get("a_bool"),
        Some(&ParameterValue::Bool(false))
    );
    assert_eq!(
        restored.values.get("an_int"),
        Some(&ParameterValue::Int(42))
    );
    assert_eq!(
        restored.values.get("a_str"),
        Some(&ParameterValue::String("hello".into()))
    );
}

// ─── serialize_project directly ───

#[test]
fn serialize_project_produces_valid_yaml_string() {
    let project = Project {
        name: Some("Direct Serialize".into()),
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: Some("ch1".into()),
            instrument: "generic".to_string(),
            enabled: false,
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
        }],
        midi: None,
    };
    let yaml_str = super::serialize_project(&project).expect("serialize");
    assert!(yaml_str.contains("name: Direct Serialize"));
    assert!(yaml_str.contains("type: input"));
    assert!(yaml_str.contains("type: output"));
}

// ─── serialize_audio_blocks directly ───

#[test]
fn serialize_audio_blocks_returns_vec_of_values() {
    let delay_model = first_model(block_delay::supported_models());
    let blocks = vec![core_block("b:0", "delay", delay_model, Vec::new())];
    let values = super::serialize_audio_blocks(&blocks).expect("serialize");
    assert_eq!(values.len(), 1);
    let yaml_str = serde_yaml::to_string(&values[0]).expect("to string");
    assert!(yaml_str.contains("type: delay"));
    assert!(yaml_str.contains(&format!("model: {}", delay_model)));
}

// ─── ChainBlocksPreset save/load with various block types ───

#[test]
fn preset_roundtrips_with_core_blocks() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("multi.yaml");
    let delay_model = first_model(block_delay::supported_models());
    let reverb_model = first_model(block_reverb::supported_models());
    let preset = ChainBlocksPreset {
        id: "multi".into(),
        name: Some("Multi Block Preset".into()),
        volume: 100.0,
        instrument: "electric_guitar".to_string(),
        blocks: vec![
            core_block("preset:multi:block:0", "delay", delay_model, Vec::new()),
            core_block("preset:multi:block:1", "reverb", reverb_model, Vec::new()),
        ],
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let loaded = load_chain_preset_file(&path).expect("load");
    assert_eq!(loaded.id, "multi");
    assert_eq!(loaded.name, Some("Multi Block Preset".into()));
    assert_eq!(loaded.blocks.len(), 2);
    assert_eq!(loaded.blocks[0].model_ref().unwrap().model, delay_model);
    assert_eq!(loaded.blocks[1].model_ref().unwrap().model, reverb_model);
}

#[test]
fn preset_roundtrips_with_no_blocks() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("empty.yaml");
    let preset = ChainBlocksPreset {
        id: "empty".into(),
        name: None,
        volume: 100.0,
        instrument: "electric_guitar".to_string(),
        blocks: Vec::new(),
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let loaded = load_chain_preset_file(&path).expect("load");
    assert_eq!(loaded.id, "empty");
    assert!(loaded.name.is_none());
    assert!(loaded.blocks.is_empty());
}

#[test]
fn preset_roundtrips_with_input_output_blocks() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("io_preset.yaml");
    let preset = ChainBlocksPreset {
        id: "io_preset".into(),
        name: Some("IO Preset".into()),
        volume: 100.0,
        instrument: "electric_guitar".to_string(),
        blocks: vec![
            AudioBlock {
                id: BlockId("preset:io_preset:block:0".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".to_string(),
                    io: "main".to_string(),
                    endpoint: "Mic In".to_string(),
                }),
            },
            AudioBlock {
                id: BlockId("preset:io_preset:block:1".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".to_string(),
                    io: "main".to_string(),
                    endpoint: "Speaker Out".to_string(),
                }),
            },
        ],
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let loaded = load_chain_preset_file(&path).expect("load");
    assert_eq!(loaded.blocks.len(), 2);
    assert!(
        matches!(&loaded.blocks[0].kind, AudioBlockKind::Input(inp) if inp.io == "main" && inp.endpoint == "Mic In")
    );
    assert!(
        matches!(&loaded.blocks[1].kind, AudioBlockKind::Output(out) if out.io == "main" && out.endpoint == "Speaker Out")
    );
}

// ─── Error cases ───

#[test]
fn load_project_fails_on_invalid_yaml() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("bad.yaml");
    fs::write(&path, "{{{{not valid yaml!!!!").expect("write");
    let repo = YamlProjectRepository { path };
    let result = repo.load_current_project();
    assert!(result.is_err());
}

#[test]
fn load_project_fails_on_missing_chains_field() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("no_chains.yaml");
    fs::write(&path, "name: Missing Chains\n").expect("write");
    let repo = YamlProjectRepository { path };
    let result = repo.load_current_project();
    assert!(result.is_err());
}

#[test]
fn load_project_fails_on_nonexistent_file() {
    let repo = YamlProjectRepository {
        path: PathBuf::from("/tmp/does_not_exist_openrig_test.yaml"),
    };
    let result = repo.load_current_project();
    assert!(result.is_err());
}

#[test]
fn load_preset_fails_on_invalid_yaml() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("bad_preset.yaml");
    fs::write(&path, ":::not yaml:::").expect("write");
    let result = load_chain_preset_file(&path);
    assert!(result.is_err());
}

// ─── yaml_scalar_to_parameter_value edge cases ───

#[test]
fn yaml_scalar_sequence_returns_error() {
    let seq = serde_yaml::Value::Sequence(vec![serde_yaml::Value::Bool(true)]);
    let result = super::yaml_scalar_to_parameter_value(seq);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unsupported yaml value"));
}

#[test]
fn yaml_key_non_string_returns_error() {
    let key = serde_yaml::Value::Bool(true);
    let result = super::yaml_key_to_string(key);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("keys must be strings"));
}

// ─── Device settings roundtrip ───

#[test]
fn device_settings_not_persisted_in_yaml() {
    use project::device::DeviceSettings;
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("with_devices.yaml");
    let repo = YamlProjectRepository { path: path.clone() };
    let project = Project {
        name: Some("With Devices".into()),
        device_settings: vec![DeviceSettings {
            device_id: DeviceId("coreaudio:builtin".into()),
            sample_rate: 48000,
            buffer_size_frames: 256,
            bit_depth: 32,
            #[cfg(target_os = "linux")]
            realtime: true,
            #[cfg(target_os = "linux")]
            rt_priority: 70,
            #[cfg(target_os = "linux")]
            nperiods: 3,
        }],
        chains: Vec::new(),
        midi: None,
    };
    repo.save_project(&project).expect("save");
    // device_settings are no longer written to YAML (per-machine config)
    let yaml_content = fs::read_to_string(&path).expect("read");
    assert!(!yaml_content.contains("device_settings"));
    let loaded = repo.load_current_project().expect("load");
    assert_eq!(loaded.device_settings.len(), 0);
}

#[test]
fn legacy_device_settings_still_deserialize() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("legacy.yaml");
    let delay_model = first_model(block_delay::supported_models());
    fs::write(&path, format!(
            "name: Legacy\ndevice_settings:\n  - device_id: \"coreaudio:builtin\"\n    sample_rate: 48000\n    buffer_size_frames: 256\nchains:\n  - description: ch1\n    instrument: electric_guitar\n    blocks:\n      - type: input\n        model: standard\n        enabled: true\n        entries:\n          - name: In\n            device_id: \"coreaudio:builtin\"\n            mode: mono\n            channels: [0]\n      - type: delay\n        model: {}\n        enabled: true\n        params:\n          time_ms: 300.0\n          feedback: 40.0\n          mix: 30.0\n      - type: output\n        model: standard\n        enabled: true\n        entries:\n          - name: Out\n            device_id: \"coreaudio:builtin\"\n            mode: stereo\n            channels: [0, 1]\n",
            delay_model
        )).expect("write");
    let repo = YamlProjectRepository { path };
    let loaded = repo.load_current_project().expect("load");
    // Legacy device_settings are still read for backward compat
    assert_eq!(loaded.device_settings.len(), 1);
    assert_eq!(
        loaded.device_settings[0].device_id,
        DeviceId("coreaudio:builtin".into())
    );
}

// ─── Default instrument ───

#[test]
fn load_project_defaults_instrument_to_electric_guitar() {
    let temp_dir = tempdir().expect("temp dir");
    let project_path = temp_dir.path().join("no_instrument.yaml");
    fs::write(
        &project_path,
        r#"
chains:
  - description: no instrument field
    blocks:
      - type: input
        enabled: true
        model: standard
        io: main
        endpoint: In 1
      - type: output
        enabled: true
        model: standard
        io: main
        endpoint: Out 1
"#,
    )
    .expect("write");
    let repo = YamlProjectRepository { path: project_path };
    let project = repo.load_current_project().expect("load");
    assert_eq!(project.chains[0].instrument, "electric_guitar");
}

// ─── Nam block roundtrip ───

#[test]
fn roundtrip_nam_block_preserves_model_and_params() {
    use domain::value_objects::ParameterValue;
    use project::block::NamBlock;
    let Some(nam_model) = first_model_optional(block_nam::supported_models()) else {
        return;
    };
    let schema = project::block::schema_for_block_model("nam", nam_model).expect("nam schema");
    let mut params = ParameterSet::default();
    params.insert("model_path", ParameterValue::String("/tmp/test.nam".into()));
    let params = params.normalized_against(&schema).expect("normalize");
    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock {
            model: nam_model.to_string(),
            params,
        }),
    };
    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
    match &restored.kind {
        AudioBlockKind::Nam(nam) => assert_eq!(nam.model, nam_model),
        other => panic!("expected Nam block, got {:?}", other),
    }
}

// ─── Multiple chains in a project ───

#[test]
fn project_with_multiple_chains_roundtrips() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("multi_chain.yaml");
    let repo = YamlProjectRepository { path: path.clone() };
    let project = Project {
        name: Some("Multi Chain".into()),
        device_settings: Vec::new(),
        chains: vec![
            Chain {
                id: ChainId("chain:0".into()),
                description: Some("Guitar".into()),
                instrument: "electric_guitar".to_string(),
                enabled: false,
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
            },
            Chain {
                id: ChainId("chain:1".into()),
                description: Some("Bass".into()),
                instrument: "bass".to_string(),
                enabled: false,
                volume: 100.0,
                io_binding_ids: vec![],
                blocks: vec![
                    AudioBlock {
                        id: BlockId("chain:1:input:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Input(InputBlock {
                            model: "standard".to_string(),
                            io: String::new(),
                            endpoint: String::new(),
                        }),
                    },
                    AudioBlock {
                        id: BlockId("chain:1:output:0".into()),
                        enabled: true,
                        kind: AudioBlockKind::Output(OutputBlock {
                            model: "standard".to_string(),
                            io: String::new(),
                            endpoint: String::new(),
                        }),
                    },
                ],
                di_output: None,
            },
        ],
        midi: None,
    };
    repo.save_project(&project).expect("save");
    let loaded = repo.load_current_project().expect("load");
    assert_eq!(loaded.chains.len(), 2);
    assert_eq!(loaded.chains[0].description, Some("Guitar".into()));
    assert_eq!(loaded.chains[0].instrument, "electric_guitar");
    assert_eq!(loaded.chains[1].description, Some("Bass".into()));
    assert_eq!(loaded.chains[1].instrument, "bass");
}

// ─── insert_yaml_value with empty path is a no-op ───

#[test]
fn insert_yaml_value_empty_path_is_noop() {
    let mut mapping = serde_yaml::Mapping::new();
    super::insert_yaml_value(&mut mapping, &[], serde_yaml::Value::Bool(true));
    assert!(mapping.is_empty());
}

// ─── Disabled block roundtrip ───

#[test]
fn disabled_core_block_preserves_enabled_false() {
    let delay_model = first_model(block_delay::supported_models());
    let mut block = core_block("chain:0:block:0", "delay", delay_model, Vec::new());
    block.enabled = false;

    let yaml = super::AudioBlockYaml::from_audio_block(&block).expect("to yaml");
    let value = serde_yaml::to_value(&yaml).expect("serialize");
    let parsed: super::AudioBlockYaml = serde_yaml::from_value(value).expect("deserialize");
    let chain_id = ChainId("chain:0".to_string());
    let restored = parsed.into_audio_block(&chain_id, 0).expect("into block");
    assert!(!restored.enabled);
}

// ─── from_audio_block with unsupported effect_type returns error ───

#[test]
fn from_audio_block_unsupported_effect_type_returns_error() {
    let block = AudioBlock {
        id: BlockId("chain:0:block:0".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "nonexistent_type".to_string(),
            model: "foo".to_string(),
            params: ParameterSet::default(),
        }),
    };
    let result = super::AudioBlockYaml::from_audio_block(&block);
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("unsupported core block effect_type"));
}

// ─── legacy native_guitar_eq → native_guitar_hpf_lpf migration (#303) ───

#[test]
fn legacy_guitar_eq_with_low_cut_migrates_to_hpf_lpf() {
    let yaml = r#"
type: filter
enabled: true
model: native_guitar_eq
params:
  low_cut: 50.0
  high_cut: 75.0
"#;
    let parsed: super::AudioBlockYaml = serde_yaml::from_str(yaml).expect("parse");
    let chain_id = ChainId("chain:0".to_string());
    let block = parsed
        .into_audio_block(&chain_id, 0)
        .expect("legacy guitar_eq should migrate");
    match &block.kind {
        AudioBlockKind::Core(core) => {
            assert_eq!(core.effect_type, "filter");
            assert_eq!(
                core.model, "native_guitar_hpf_lpf",
                "legacy native_guitar_eq with low_cut/high_cut params should remap"
            );
        }
        other => panic!("expected Core block, got {other:?}"),
    }
}

#[test]
fn legacy_guitar_eq_with_only_high_cut_migrates_to_hpf_lpf() {
    let yaml = r#"
type: filter
enabled: true
model: native_guitar_eq
params:
  high_cut: 80.0
"#;
    let parsed: super::AudioBlockYaml = serde_yaml::from_str(yaml).expect("parse");
    let chain_id = ChainId("chain:0".to_string());
    let block = parsed
        .into_audio_block(&chain_id, 0)
        .expect("legacy guitar_eq should migrate");
    match &block.kind {
        AudioBlockKind::Core(core) => assert_eq!(core.model, "native_guitar_hpf_lpf"),
        other => panic!("expected Core block, got {other:?}"),
    }
}

#[test]
fn new_guitar_eq_with_band_gains_keeps_id() {
    let yaml = r#"
type: filter
enabled: true
model: native_guitar_eq
params:
  low: 0.0
  low_mid: 0.0
  high_mid: 0.0
  high: 0.0
"#;
    let parsed: super::AudioBlockYaml = serde_yaml::from_str(yaml).expect("parse");
    let chain_id = ChainId("chain:0".to_string());
    let block = parsed
        .into_audio_block(&chain_id, 0)
        .expect("new guitar_eq should load as-is");
    match &block.kind {
        AudioBlockKind::Core(core) => assert_eq!(
            core.model, "native_guitar_eq",
            "new params (low/low_mid/high_mid/high) must NOT trigger the legacy remap"
        ),
        other => panic!("expected Core block, got {other:?}"),
    }
}

// ─── Chain volume round-trip (issue #440) ───

/// PIN: Chain.volume=150 survives save → load with the exact value.
#[test]
fn chain_volume_150_roundtrips_through_yaml() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("vol_roundtrip.yaml");
    let repo = YamlProjectRepository { path: path.clone() };
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 150.0,
            io_binding_ids: vec![],
            blocks: Vec::new(),
            di_output: None,
        }],
        midi: None,
    };
    repo.save_project(&project).expect("save should succeed");
    let loaded = repo.load_current_project().expect("load should succeed");
    assert_eq!(
        loaded.chains[0].volume, 150.0,
        "volume=150 must survive YAML save+load round-trip"
    );
}

/// #716: a chain's selected I/O binding ids survive the YAML round-trip.
#[test]
fn chain_io_binding_ids_roundtrip_through_yaml() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("io_binding_ids_roundtrip.yaml");
    let repo = YamlProjectRepository { path: path.clone() };
    let project = Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![Chain {
            id: ChainId("chain:0".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["main".into(), "fx".into()],
            blocks: Vec::new(),
            di_output: None,
        }],
        midi: None,
    };
    repo.save_project(&project).expect("save should succeed");
    let loaded = repo.load_current_project().expect("load should succeed");
    assert_eq!(
        loaded.chains[0].io_binding_ids,
        vec!["main".to_string(), "fx".to_string()],
        "io_binding_ids must survive YAML save+load round-trip"
    );
}

/// #716: legacy YAML without `io_binding_ids` defaults to empty (back-compat).
#[test]
fn chain_io_binding_ids_missing_defaults_to_empty() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("legacy_no_io_bindings.yaml");
    let yaml = "\
chains:
  - instrument: electric_guitar
    enabled: true
    blocks: []
";
    fs::write(&path, yaml).expect("write legacy yaml");
    let repo = YamlProjectRepository { path };
    let loaded = repo.load_current_project().expect("load legacy yaml");
    assert!(
        loaded.chains[0].io_binding_ids.is_empty(),
        "missing io_binding_ids key in legacy YAML must default to empty"
    );
}

/// PIN: legacy YAML without a `volume:` key must default to 100.0
/// (backward compatibility — existing user projects are unaffected).
#[test]
fn chain_volume_missing_in_yaml_defaults_to_100() {
    let temp_dir = tempdir().expect("temp dir");
    let path = temp_dir.path().join("legacy_no_volume.yaml");
    let yaml = "\
chains:
  - instrument: electric_guitar
    enabled: true
    input:
      - device_id: dev
        mode: mono
        channels: [0]
    output:
      - device_id: dev
        mode: stereo
        channels: [0, 1]
    blocks: []
";
    fs::write(&path, yaml).expect("write legacy yaml");
    let repo = YamlProjectRepository { path };
    let loaded = repo.load_current_project().expect("load legacy yaml");
    assert_eq!(
        loaded.chains[0].volume, 100.0,
        "missing volume key in legacy YAML must default to 100.0"
    );
}

#[test]
fn load_project_preserves_chain_volume_from_yaml() {
    // Reproduz o bug do user (issue #440): chain YAML tem `volume: 175`
    // mas o `into_chain()` hardcodava `volume: 100.0`, fazendo o valor
    // do disco ser ignorado. Toda re-carga do projeto resetava o slider.
    let temp_dir = tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("project.yaml");
    fs::write(
        &project_path,
        r#"
chains:
  - enabled: true
    instrument: electric_guitar
    volume: 175
    blocks: []
"#,
    )
    .expect("write project");
    let repo = YamlProjectRepository { path: project_path };
    let project = repo.load_current_project().expect("load: chain.volume");
    assert_eq!(project.chains.len(), 1, "should parse the single chain");
    assert!(
        (project.chains[0].volume - 175.0).abs() < 0.01,
        "chain.volume from YAML should be preserved; got {}",
        project.chains[0].volume
    );
}

#[test]
fn load_project_defaults_chain_volume_to_100_when_absent() {
    // Sem `volume:` no YAML, default = 100 (unity). Confirma compat
    // pra projetos pré-#440 que não têm o field.
    let temp_dir = tempdir().expect("temp dir should be created");
    let project_path = temp_dir.path().join("project.yaml");
    fs::write(
        &project_path,
        r#"
chains:
  - enabled: true
    instrument: electric_guitar
    blocks: []
"#,
    )
    .expect("write project");
    let repo = YamlProjectRepository { path: project_path };
    let project = repo
        .load_current_project()
        .expect("load: chain.volume default");
    assert_eq!(project.chains.len(), 1);
    assert!(
        (project.chains[0].volume - 100.0).abs() < 0.01,
        "absent volume should default to 100; got {}",
        project.chains[0].volume
    );
}

// ── #450 preset YAML versioning + legacy preset → RigPreset ───────────────

#[test]
fn legacy_preset_without_version_still_loads() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("old.yaml");
    fs::write(&path, "id: old\nvolume: 120.0\nblocks: []\n").unwrap();
    // No `version:` key (pre-version preset) ⇒ loads, volume preserved.
    let p = load_chain_preset_file(&path).expect("pre-version preset loads");
    assert_eq!(p.id, "old");
    assert_eq!(p.volume, 120.0);
}

#[test]
fn preset_save_writes_version() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("p.yaml");
    let preset = ChainBlocksPreset {
        id: "p".into(),
        name: None,
        volume: 100.0,
        instrument: "electric_guitar".to_string(),
        blocks: Vec::new(),
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let raw = fs::read_to_string(&path).unwrap();
    assert!(
        raw.contains(&format!("version: {}", project::rig::PRESET_FORMAT_VERSION)),
        "preset file must stamp version, got:\n{raw}"
    );
}

#[test]
fn preset_rejects_future_version() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("future.yaml");
    fs::write(&path, "version: 999\nid: f\nblocks: []\n").unwrap();
    let err = load_chain_preset_file(&path).unwrap_err().to_string();
    assert!(
        err.contains("999") && err.to_lowercase().contains("newer"),
        "future preset version must be refused, got: {err}"
    );
}

#[test]
fn legacy_preset_loads_as_rig_preset_blocks_and_volume_preserved() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("lead.yaml");
    let delay_model = first_model(block_delay::supported_models());
    let preset = ChainBlocksPreset {
        id: "lead".into(),
        name: Some("Lead Tone".into()),
        volume: 142.0,
        instrument: "electric_guitar".to_string(),
        blocks: vec![core_block(
            "preset:lead:block:0",
            "delay",
            delay_model,
            Vec::new(),
        )],
    };
    save_chain_preset_file(&path, &preset).expect("save");

    let (name, rig) = super::load_legacy_preset_as_rig(&path).expect("convert");

    assert_eq!(name, "Lead Tone", "human name preferred over id");
    assert_eq!(rig.volume, 142.0, "volume preserved exact");
    assert_eq!(rig.blocks.len(), 1);
    assert_eq!(rig.blocks[0].model_ref().unwrap().model, delay_model);
    assert!(rig.scenes.is_empty(), "no scenes (back-compat default)");
    assert!(rig.scene_params.is_empty());
}

#[test]
fn legacy_preset_without_name_uses_id() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("raw.yaml");
    let preset = ChainBlocksPreset {
        id: "raw-id".into(),
        name: None,
        volume: 100.0,
        instrument: "electric_guitar".to_string(),
        blocks: Vec::new(),
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let (name, _) = super::load_legacy_preset_as_rig(&path).expect("convert");
    assert_eq!(name, "raw-id");
}
