//! Round-trip + file I/O tests for the project `.yaml` parser (#449, #1014).

use super::*;

const MINIMAL: &str = r#"
project:
  name: Studio
  inputs:
    input-1:
      label: Eu + filho
      io: main
      endpoint: Guitar In
      bank:
        1: clean
        2: drive
      active-preset: 1
      active-scene: 1
      routing: [out-1]
  outputs:
    out-1:
      label: PA L
      io: main
      endpoint: Monitor Out
  presets:
    clean:
      blocks: []
    drive:
      blocks: []
"#;

#[test]
fn parse_minimal_ok() {
    let p = parse_project(MINIMAL).expect("should parse");
    assert_eq!(p.name.as_deref(), Some("Studio"));
    let input = p.inputs.get("input-1").expect("input-1");
    assert_eq!(input.io, "main", "binding id preserved");
    assert_eq!(input.endpoint, "Guitar In", "endpoint preserved");
    assert_eq!(input.bank.get(&2).map(String::as_str), Some("drive"));
    assert_eq!(input.active_preset, 1);
    assert!(p.outputs.contains_key("out-1"));
    assert_eq!(p.presets.len(), 2);
}

#[test]
fn serialize_writes_current_version() {
    let p = parse_project(MINIMAL).unwrap();
    let s = serialize_project(&p).unwrap();
    assert!(
        s.contains(&format!(
            "version: {}",
            project::rig::PROJECT_FORMAT_VERSION
        )),
        "serialized doc must carry the format version, got:\n{s}"
    );
}

#[test]
fn parse_without_version_defaults_to_current() {
    // MINIMAL has no `version:` key (pre-version file) ⇒ loads as v1.
    let p = parse_project(MINIMAL).expect("pre-version doc still loads");
    assert_eq!(p.presets.len(), 2);
}

#[test]
fn parse_rejects_future_version() {
    let future = format!("version: 999\n{MINIMAL}");
    let err = parse_project(&future).unwrap_err().to_string();
    assert!(
        err.contains("999") && err.to_lowercase().contains("newer"),
        "future version must be refused cleanly, got: {err}"
    );
}

#[test]
fn round_trip_deterministic() {
    let p1 = parse_project(MINIMAL).unwrap();
    let s1 = serialize_project(&p1).unwrap();
    let p2 = parse_project(&s1).unwrap();
    let s2 = serialize_project(&p2).unwrap();
    assert_eq!(s1, s2, "serialize must be byte-deterministic");
    assert_eq!(p1, p2, "round-trip must preserve the model");
}

#[test]
fn loopers_round_trip_via_yaml() {
    // #323: the user's exact symptom — "fecho o projeto e o looper vai para
    // o espaço". A looper stored on a RigInput must survive serialize+parse.
    use project::chain::LooperConfig;
    let mut p = parse_project(MINIMAL).unwrap();
    p.inputs.get_mut("input-1").unwrap().loopers = vec![LooperConfig {
        mix: 0.8,
        audio_file: Some("input-1.7.wav".into()),
        ..LooperConfig::new(7)
    }];

    let yaml = serialize_project(&p).expect("serialize ok");
    let reloaded = parse_project(&yaml).expect("parse ok");

    assert_eq!(
        reloaded.inputs["input-1"].loopers,
        vec![LooperConfig {
            mix: 0.8,
            audio_file: Some("input-1.7.wav".into()),
            ..LooperConfig::new(7)
        }],
        "a looper persisted on the rig input must survive save+reload"
    );
}

#[test]
fn parse_rejects_invalid() {
    let bad = MINIMAL.replace("1: clean", "1: ghost");
    let err = parse_project(&bad).unwrap_err().to_string();
    assert!(err.contains("ghost"), "got: {err}");
}

#[test]
fn chain_order_round_trips_via_yaml() {
    // Issue #502: a user reorder must survive save+reload. The kebab-case
    // `chain-order:` key keeps the convention used by `active-preset` etc.
    let mut p = parse_project(MINIMAL).unwrap();
    p.chain_order = vec!["input-1".to_string()];

    let yaml = serialize_project(&p).expect("serialize ok");
    assert!(
        yaml.contains("chain-order:"),
        "chain-order must appear in YAML when non-empty, got:\n{yaml}"
    );

    let reloaded = parse_project(&yaml).expect("parse ok");
    assert_eq!(
        reloaded.chain_order,
        vec!["input-1".to_string()],
        "chain_order must survive the YAML round-trip"
    );
}

#[test]
fn empty_chain_order_is_omitted_from_yaml() {
    // Back-compat: older project files have no `chain-order:` key.
    // We must keep the field out of fresh serialisations too so the
    // wire shape doesn't grow without reason.
    let p = parse_project(MINIMAL).unwrap();
    let yaml = serialize_project(&p).unwrap();
    assert!(
        !yaml.contains("chain-order"),
        "an empty chain_order must be skipped from YAML, got:\n{yaml}"
    );
}

#[test]
fn save_then_load_file_round_trips() {
    let p = parse_project(MINIMAL).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("project.yaml");
    save_project_file(&path, &p).unwrap();
    let loaded = load_project_file(&path).unwrap();
    assert_eq!(p, loaded);
}

#[test]
fn load_file_rejects_future_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.yaml");
    std::fs::write(&path, format!("version: 999\n{MINIMAL}")).unwrap();
    let err = format!("{:#}", load_project_file(&path).unwrap_err());
    assert!(err.contains("999"), "got: {err}");
}

#[test]
fn load_file_refuses_a_document_without_the_project_key() {
    // #1014: there is one project format. A bare `chains:` document is not
    // it, and must fail loudly instead of being converted on the fly.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.yaml");
    std::fs::write(&path, "name: Studio\nchains: []\n").unwrap();
    assert!(load_project_file(&path).is_err());
}

// ── #454 T3: scenes / scene-params persistence + backward-compat ───────────

const WITH_SCENES: &str = r#"
project:
  inputs:
    input-1:
      io: main
      endpoint: Guitar In
      bank:
        1: drive
      active-preset: 1
      active-scene: 2
  outputs: {}
  presets:
    drive:
      blocks: []
      scene-params:
        - od.gain
      scenes:
        1:
          bypass: {}
          params: { od.gain: 0.4 }
        2:
          label: solo
          bypass: { comp: true }
          params: { od.gain: 0.8 }
"#;

#[test]
fn scenes_and_scene_params_round_trip_deterministic() {
    let p1 = parse_project(WITH_SCENES).unwrap();
    let preset = p1.presets.get("drive").unwrap();
    assert_eq!(preset.scene_params, vec!["od.gain".to_string()]);
    assert_eq!(preset.scenes.len(), 2);
    assert_eq!(preset.scenes[&2].label.as_deref(), Some("solo"));
    assert_eq!(preset.scenes[&2].bypass.get("comp"), Some(&true));
    assert_eq!(preset.scenes[&2].params.get("od.gain"), Some(&0.8));

    let s1 = serialize_project(&p1).unwrap();
    let p2 = parse_project(&s1).unwrap();
    assert_eq!(s1, serialize_project(&p2).unwrap(), "byte-deterministic");
    assert_eq!(p1, p2);
}

#[test]
fn preset_without_scenes_loads_as_default_scene() {
    // MINIMAL has presets with `blocks: []` and no scenes ⇒ backward-compat:
    // scene_or_default(1) is the empty Default scene.
    let p = parse_project(MINIMAL).unwrap();
    let clean = p.presets.get("clean").unwrap();
    assert!(clean.scenes.is_empty());
    assert!(clean.scene_params.is_empty());
    assert_eq!(clean.scene_or_default(1), project::rig::RigScene::default());
}

// Issue #535 — save + reopen of a 2-preset bank must keep scenes per preset.
// Repro: chain has presets A (slot 1, 2 scenes) and B (slot 2, 1 scene).
// Round-tripping through serialize_project + parse_project must
// leave B with exactly its own 1 scene — no leak from A.
#[test]
fn round_trip_keeps_scenes_isolated_per_preset_in_the_same_bank() {
    use project::rig::{RigInput, RigPreset, RigProject, RigScene};
    use std::collections::BTreeMap;

    let mut preset_a = RigPreset {
        id: "a".into(),
        name: Some("A".into()),
        blocks: Vec::new(),
        scene_params: Vec::new(),
        scenes: BTreeMap::from([(1, RigScene::default()), (2, RigScene::default())]),
        volume: 100.0,
    };
    preset_a.scenes.entry(1).or_default();
    let preset_b = RigPreset {
        id: "b".into(),
        name: Some("B".into()),
        blocks: Vec::new(),
        scene_params: Vec::new(),
        scenes: BTreeMap::from([(1, RigScene::default())]),
        volume: 100.0,
    };

    let mut presets = BTreeMap::new();
    presets.insert("a".into(), preset_a);
    presets.insert("b".into(), preset_b);

    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".into(),
        RigInput {
            label: None,
            bank: BTreeMap::from([(1, "a".into()), (2, "b".into())]),
            active_preset: 1,
            active_scene: 1,
            routing: Vec::new(),
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
            loopers: Vec::new(),
            disabled_endpoints: Default::default(),
            mix: Default::default(),
            disabled_endpoints: Default::default(),
        },
    );
    let rig = RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    };

    let yaml = serialize_project(&rig).expect("serialize");
    let restored = parse_project(&yaml).expect("parse");

    assert_eq!(
        restored.presets["a"].scene_count(),
        2,
        "preset A round-trips with 2 scenes",
    );
    assert_eq!(
        restored.presets["b"].scene_count(),
        1,
        "preset B must round-trip with exactly 1 scene — no leak from A",
    );
}
