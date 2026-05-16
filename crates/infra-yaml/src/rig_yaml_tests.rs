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
