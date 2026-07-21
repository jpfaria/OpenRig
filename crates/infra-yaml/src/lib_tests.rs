//! Tests for `infra-yaml`. Lifted out of `lib.rs` so the production file
//! stays under the size cap. Re-attached as `mod tests` of the parent via
//! `#[cfg(test)] #[path = "lib_tests.rs"] mod tests;` — content kept at
//! the original 4-space indent so raw-string YAML literals stay intact.

use super::{
    load_chain_preset_file, save_chain_preset_file, ChainBlocksPreset, YamlProjectRepository,
};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock, SelectBlock};
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

pub(super) fn core_block(
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

pub(super) fn first_model<'a>(models: &'a [&'a str]) -> &'a str {
    models
        .first()
        .expect("block crate must expose at least one model")
}

/// Same as `first_model` but returns `None` when the block crate exposes
/// no native models. Used by roundtrip tests for block crates whose only
/// models live in plugin-loader / disk packages — `block-body`, `block-ir`,
/// `block-pitch`, `block-nam` — so the test no-ops instead of panicking.
pub(super) fn first_model_optional<'a>(models: &'a [&'a str]) -> Option<&'a str> {
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

