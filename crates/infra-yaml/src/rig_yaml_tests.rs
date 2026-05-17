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
fn serialize_writes_current_version() {
    let p = parse_rig_project(MINIMAL).unwrap();
    let s = serialize_rig_project(&p).unwrap();
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
    let p = parse_rig_project(MINIMAL).expect("pre-version doc still loads");
    assert_eq!(p.presets.len(), 2);
}

#[test]
fn parse_rejects_future_version() {
    let future = format!("version: 999\n{MINIMAL}");
    let err = parse_rig_project(&future).unwrap_err().to_string();
    assert!(
        err.contains("999") && err.to_lowercase().contains("newer"),
        "future version must be refused cleanly, got: {err}"
    );
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

// ── #450 legacy file migration orchestrator ───────────────────────────────

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::project::Project;

fn legacy_chain(desc: &str, vol: f32) -> Chain {
    Chain {
        id: ChainId("chain:0".into()),
        description: Some(desc.into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: vol,
        blocks: vec![
            AudioBlock {
                id: BlockId("in".into()),
                enabled: true,
                kind: AudioBlockKind::Input(InputBlock {
                    model: "standard".into(),
                    entries: vec![InputEntry {
                        device_id: DeviceId("sc".into()),
                        mode: ChainInputMode::Mono,
                        channels: vec![0],
                    }],
                }),
            },
            AudioBlock {
                id: BlockId("fx".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "delay".into(),
                    model: "tape".into(),
                    params: ParameterSet::default(),
                }),
            },
            AudioBlock {
                id: BlockId("out".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    entries: vec![OutputEntry {
                        device_id: DeviceId("sc".into()),
                        mode: ChainOutputMode::Stereo,
                        channels: vec![0, 1],
                    }],
                }),
            },
        ],
    }
}

fn write_legacy(dir: &std::path::Path, chains: Vec<Chain>) -> std::path::PathBuf {
    let project = Project {
        name: Some("Studio".into()),
        device_settings: vec![],
        chains,
    };
    let path = dir.join("project.yaml");
    std::fs::write(&path, crate::serialize_project(&project).unwrap()).unwrap();
    path
}

#[test]
fn migrate_file_creates_target_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = write_legacy(dir.path(), vec![legacy_chain("clean", 130.0)]);
    let out = dir.path().join("project.openrig");

    let rig = migrate_legacy_project_file(&legacy, &out).unwrap();

    assert_eq!(rig.presets.get("clean").unwrap().volume, 130.0);
    assert!(out.exists(), "project.openrig written");
    assert_eq!(load_rig_project_file(&out).unwrap(), rig);
    assert!(
        dir.path().join("project.yaml.bak").exists(),
        "legacy backed up"
    );
}

#[test]
fn migrate_file_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = write_legacy(dir.path(), vec![legacy_chain("clean", 100.0)]);
    let out = dir.path().join("project.openrig");

    let first = migrate_legacy_project_file(&legacy, &out).unwrap();
    let bak = std::fs::read_to_string(dir.path().join("project.yaml.bak")).unwrap();
    let second = migrate_legacy_project_file(&legacy, &out).unwrap();

    assert_eq!(first, second, "second run yields identical project");
    assert_eq!(
        bak,
        std::fs::read_to_string(dir.path().join("project.yaml.bak")).unwrap(),
        "backup not rewritten on the idempotent re-run"
    );
}

#[test]
fn migrate_file_does_not_clobber_existing_target() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = write_legacy(dir.path(), vec![legacy_chain("fromlegacy", 100.0)]);
    let out = dir.path().join("project.openrig");
    // Pre-existing valid target with different content.
    let preexisting = parse_rig_project(MINIMAL).unwrap();
    save_rig_project_file(&out, &preexisting).unwrap();

    let returned = migrate_legacy_project_file(&legacy, &out).unwrap();

    assert_eq!(returned, preexisting, "existing target preserved as-is");
    assert!(
        !dir.path().join("project.yaml.bak").exists(),
        "legacy untouched when target already valid"
    );
}

// ── #450 transparent load (new format or auto-migrated legacy) ────────────

#[test]
fn load_project_any_reads_new_openrig() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.openrig");
    let p = parse_rig_project(MINIMAL).unwrap();
    save_rig_project_file(&path, &p).unwrap();

    let loaded = load_project_any(&path).expect("new format loads");
    assert_eq!(loaded, p);
}

#[test]
fn load_project_any_migrates_legacy_transparently_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = write_legacy(dir.path(), vec![legacy_chain("clean", 133.0)]);

    let rig = load_project_any(&legacy).expect("legacy migrates on load");
    assert_eq!(rig.presets.get("clean").unwrap().volume, 133.0);

    let sibling = dir.path().join("project.openrig");
    assert!(sibling.exists(), "sibling project.openrig written");
    assert!(
        dir.path().join("project.yaml.bak").exists(),
        "legacy backed up"
    );

    let again = load_project_any(&legacy).expect("idempotent");
    assert_eq!(rig, again, "second load yields identical project");
}

#[test]
fn load_project_any_rejects_future_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("project.openrig");
    std::fs::write(&path, format!("version: 999\n{MINIMAL}")).unwrap();
    let err = load_project_any(&path).unwrap_err().to_string();
    assert!(err.contains("999"), "got: {err}");
}
