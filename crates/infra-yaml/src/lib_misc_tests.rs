//! NAM/error-case/preset tests (issue #792 split from lib_tests.rs).

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::param::ParameterSet;
use std::fs;
use tempfile::tempdir;

use super::tests::{core_block, first_model, first_model_optional};
use super::*;

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
