//! Serialize/roundtrip + edge-case tests (issue #792 split from lib_tests.rs).

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

use super::tests::{core_block, first_model};
use super::*;

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

