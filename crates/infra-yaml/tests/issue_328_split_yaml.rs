//! #328 — persistence of a chain split (spec §2).
//!
//! `project.openrig` carries the split through the derive (`kind: !Split`).
//! Chain presets and legacy project files carry it as `type: split` with
//! positional path blocks, loaded as `<split>::a:<i>` / `<split>::b:<i>`. A
//! document that holds a split is `version: 2`; a split-free one stays at
//! `version: 1`, so an older build keeps opening it.

use std::collections::BTreeMap;
use std::fs;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use infra_yaml::{
    load_chain_preset_file, parse_rig_project, save_chain_preset_file, serialize_rig_project,
    ChainBlocksPreset, YamlProjectRepository,
};
use project::block::split_params::{MIX_PAN_A, MIX_PAN_B};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};
use tempfile::tempdir;

fn delay_model() -> String {
    block_delay::supported_models()
        .first()
        .expect("a native delay model")
        .to_string()
}

fn delay(id: &str) -> AudioBlock {
    let model = delay_model();
    let schema = project::block::schema_for_block_model("delay", &model).expect("delay schema");
    let params = ParameterSet::default()
        .normalized_against(&schema)
        .expect("delay defaults");
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model,
            params,
        }),
    }
}

/// Amp A panned hard left, amp B hard right — the main use of Split → Mix.
fn dual_amp_split(id: &str) -> AudioBlock {
    let mut split = SplitBlock::new(SplitEnd::Mix);
    split.a = vec![delay(&format!("{id}::a:0"))];
    split.b = vec![delay(&format!("{id}::b:0"))];
    split.params.insert(MIX_PAN_A, ParameterValue::Float(-50.0));
    split.params.insert(MIX_PAN_B, ParameterValue::Float(50.0));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(split),
    }
}

fn rig_with(blocks: Vec<AudioBlock>) -> RigProject {
    RigProject {
        name: Some("Split".into()),
        inputs: BTreeMap::from([(
            "g".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "p".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".into(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
                disabled_endpoints: Default::default(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "p".to_string(),
            RigPreset::from_legacy_blocks(blocks, 100.0),
        )]),
        midi: None,
        chain_order: Vec::new(),
    }
}

fn split_of(block: &AudioBlock) -> &SplitBlock {
    match &block.kind {
        AudioBlockKind::Split(s) => s,
        other => panic!("expected a split, got {}", other.label()),
    }
}

#[test]
fn a_project_with_a_split_round_trips_and_is_stamped_version_2() {
    let rig = rig_with(vec![delay("pre"), dual_amp_split("split"), delay("post")]);
    let yaml = serialize_rig_project(&rig).expect("serialize");
    assert!(
        yaml.contains("!Split"),
        "the split is a tagged kind, got:\n{yaml}"
    );
    assert!(
        yaml.contains("version: 2\n"),
        "a file holding a split is version 2, got:\n{yaml}"
    );
    let back = parse_rig_project(&yaml).expect("a version 2 file loads in this build");
    assert_eq!(
        back, rig,
        "every path block and knob survives the round trip"
    );
}

#[test]
fn a_split_free_project_stays_version_1() {
    let yaml = serialize_rig_project(&rig_with(vec![delay("pre")])).expect("serialize");
    assert!(
        yaml.contains("version: 1\n") && !yaml.contains("version: 2"),
        "no split, no bump — older builds keep opening it, got:\n{yaml}"
    );
}

#[test]
fn this_build_reads_version_2_and_refuses_version_3() {
    let v1 = serialize_rig_project(&rig_with(vec![delay("pre")])).expect("serialize");
    parse_rig_project(&v1.replacen("version: 1", "version: 2", 1)).expect("version 2 is readable");
    let err = parse_rig_project(&v1.replacen("version: 1", "version: 3", 1))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("newer") && err.contains("max 2"),
        "a newer file is refused cleanly, got: {err}"
    );
}

#[test]
fn a_chain_preset_writes_type_split_with_positional_path_blocks() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("dual.yaml");
    let preset = ChainBlocksPreset {
        id: "dual".into(),
        name: Some("Dual".into()),
        volume: 100.0,
        instrument: "electric_guitar".into(),
        blocks: vec![dual_amp_split("whatever")],
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let raw = fs::read_to_string(&path).expect("read back");
    assert!(
        raw.contains("type: split") && raw.contains("version: 2"),
        "got:\n{raw}"
    );

    let loaded = load_chain_preset_file(&path).expect("load");
    let split = split_of(&loaded.blocks[0]);
    assert_eq!(loaded.blocks[0].id.0, "preset:dual:block:0");
    assert_eq!(split.a[0].id.0, "preset:dual:block:0::a:0");
    assert_eq!(split.b[0].id.0, "preset:dual:block:0::b:0");
    assert_eq!(split.end, SplitEnd::Mix);
    assert_eq!(split.params.get_f32(MIX_PAN_A), Some(-50.0));
    assert_eq!(split.params.get_f32(MIX_PAN_B), Some(50.0));
}

#[test]
fn a_path_block_this_machine_cannot_load_drops_only_that_block() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("partial.yaml");
    let model = delay_model();
    // No `version:` key on purpose: this pins the per-block drop inside a
    // path, not the version gate (which `this_build_reads_version_2…` and
    // `a_chain_preset_writes_type_split…` pin).
    fs::write(
        &path,
        format!(
            "id: partial\nblocks:\n  - type: split\n    end: mix\n    a:\n      - type: delay\n        model: {model}\n      - type: delay\n        model: no_such_delay_328\n    b:\n      - type: delay\n        model: {model}\n"
        ),
    )
    .expect("write");

    let loaded = load_chain_preset_file(&path).expect("load");
    assert_eq!(loaded.blocks.len(), 1, "the split itself survives");
    let split = split_of(&loaded.blocks[0]);
    let a_ids: Vec<&str> = split.a.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(
        a_ids,
        vec!["preset:partial:block:0::a:0"],
        "only the block this machine cannot load is dropped"
    );
    assert_eq!(split.b.len(), 1, "path B is untouched");
}

#[test]
fn a_legacy_project_file_reads_type_split() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("legacy.yaml");
    let model = delay_model();
    fs::write(
        &path,
        format!(
            "chains:\n  - instrument: electric_guitar\n    blocks:\n      - type: split\n        end: y\n        a:\n          - type: delay\n            model: {model}\n        b: []\n"
        ),
    )
    .expect("write");
    let project = YamlProjectRepository { path }
        .load_current_project()
        .expect("legacy load");
    let split = split_of(&project.chains[0].blocks[0]);
    assert_eq!(split.end, SplitEnd::Y);
    assert_eq!(split.a[0].id.0, "chain:0:block:0::a:0");
}
