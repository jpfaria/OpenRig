//! Round-trip + file I/O tests for the `project.openrig` parser (#449).

use super::*;

const MINIMAL: &str = r#"
project:
  name: Studio
  inputs:
    input-1:
      label: Eu + filho
      sources:
        - device_id: scarlett
          mode: mono
          channels: [0]
        - device_id: scarlett
          mode: mono
          channels: [1]
      bank:
        1: clean
        2: drive
      active-preset: 1
      active-scene: 1
      routing: [out-1]
  outputs:
    out-1:
      label: PA L
      device_id: scarlett
      mode: stereo
      channels: [0, 1]
  presets:
    clean:
      blocks: []
    drive:
      blocks: []
"#;

#[test]
fn parse_minimal_ok() {
    let p = parse_rig_project(MINIMAL).expect("should parse");
    assert_eq!(p.name.as_deref(), Some("Studio"));
    let input = p.inputs.get("input-1").expect("input-1");
    assert_eq!(input.sources.len(), 2, "multi-source preserved");
    assert_eq!(input.bank.get(&2).map(String::as_str), Some("drive"));
    assert_eq!(input.active_preset, 1);
    assert!(p.outputs.contains_key("out-1"));
    assert_eq!(p.presets.len(), 2);
}

#[test]
fn round_trip_deterministic() {
    let p1 = parse_rig_project(MINIMAL).unwrap();
    let s1 = serialize_rig_project(&p1).unwrap();
    let p2 = parse_rig_project(&s1).unwrap();
    let s2 = serialize_rig_project(&p2).unwrap();
    assert_eq!(s1, s2, "serialize must be byte-deterministic");
    assert_eq!(p1, p2, "round-trip must preserve the model");
}

#[test]
fn parse_rejects_invalid() {
    let bad = MINIMAL.replace("1: clean", "1: ghost");
    let err = parse_rig_project(&bad).unwrap_err().to_string();
    assert!(err.contains("ghost"), "got: {err}");
}

#[test]
fn save_then_load_file_round_trips() {
    let p = parse_rig_project(MINIMAL).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub").join("project.openrig");
    save_rig_project_file(&path, &p).unwrap();
    let loaded = load_rig_project_file(&path).unwrap();
    assert_eq!(p, loaded);
}

// ── #454 T3: scenes / scene-params persistence + backward-compat ───────────

const WITH_SCENES: &str = r#"
project:
  inputs:
    input-1:
      sources:
        - device_id: sc
          mode: mono
          channels: [0]
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
    let p1 = parse_rig_project(WITH_SCENES).unwrap();
    let preset = p1.presets.get("drive").unwrap();
    assert_eq!(preset.scene_params, vec!["od.gain".to_string()]);
    assert_eq!(preset.scenes.len(), 2);
    assert_eq!(preset.scenes[&2].label.as_deref(), Some("solo"));
    assert_eq!(preset.scenes[&2].bypass.get("comp"), Some(&true));
    assert_eq!(preset.scenes[&2].params.get("od.gain"), Some(&0.8));

    let s1 = serialize_rig_project(&p1).unwrap();
    let p2 = parse_rig_project(&s1).unwrap();
    assert_eq!(
        s1,
        serialize_rig_project(&p2).unwrap(),
        "byte-deterministic"
    );
    assert_eq!(p1, p2);
}

#[test]
fn preset_without_scenes_loads_as_default_scene() {
    // MINIMAL has presets with `blocks: []` and no scenes ⇒ backward-compat:
    // scene_or_default(1) is the empty Default scene.
    let p = parse_rig_project(MINIMAL).unwrap();
    let clean = p.presets.get("clean").unwrap();
    assert!(clean.scenes.is_empty());
    assert!(clean.scene_params.is_empty());
    assert_eq!(clean.scene_or_default(1), project::rig::RigScene::default());
}
