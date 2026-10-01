//! #328 — persistence of a chain split (spec §2).
//!
//! The project file carries the split through the derive (`kind: !Split`).
//! Chain presets carry it as `type: split` with
//! positional path blocks, loaded as `<split>::p0:<i>` / `<split>::p1:<i>`. A
//! document that holds a split is `version: 2`; a split-free one stays at
//! `version: 1`, so an older build keeps opening it.

use std::collections::BTreeMap;
use std::fs;

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use infra_yaml::{
    load_chain_preset_file, parse_project, save_chain_preset_file, serialize_project,
    ChainBlocksPreset,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, SplitBlock, SplitEnd};
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};
use tempfile::tempdir;

/// The pans of paths A and B on the mixer.
const MIX_PAN_A: &str = "mix_pan_0";
const MIX_PAN_B: &str = "mix_pan_1";

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
    split.paths[0] = vec![delay(&format!("{id}::p0:0"))];
    split.paths[1] = vec![delay(&format!("{id}::p1:0"))];
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
                mix: Default::default(),
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
    let yaml = serialize_project(&rig).expect("serialize");
    assert!(
        yaml.contains("!Split"),
        "the split is a tagged kind, got:\n{yaml}"
    );
    assert!(
        yaml.contains("version: 2\n"),
        "a file holding a split is version 2, got:\n{yaml}"
    );
    let back = parse_project(&yaml).expect("a version 2 file loads in this build");
    assert_eq!(
        back, rig,
        "every path block and knob survives the round trip"
    );
}

/// A cab IR on path A, nothing on path B — the Y that ends a Mix + Y chain.
fn cab_or_dry_y_split(id: &str) -> AudioBlock {
    let mut split = SplitBlock::new(SplitEnd::Y);
    split.paths[0] = vec![delay(&format!("{id}::p0:0"))];
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Split(split),
    }
}

#[test]
fn a_project_with_a_mix_then_a_y_round_trips() {
    let rig = rig_with(vec![
        delay("pre"),
        dual_amp_split("mix"),
        delay("mid"),
        cab_or_dry_y_split("y"),
    ]);
    let yaml = serialize_project(&rig).expect("serialize");
    assert!(yaml.contains("version: 2\n"), "got:\n{yaml}");
    let back = parse_project(&yaml).expect("a Mix then a Y loads");
    assert_eq!(back, rig, "both splits and their paths survive");
    let blocks = &back.presets["p"].blocks;
    assert_eq!(split_of(&blocks[1]).paths[0][0].id.0, "mix::p0:0");
    assert_eq!(split_of(&blocks[3]).paths[0][0].id.0, "y::p0:0");
}

#[test]
fn a_chain_preset_with_a_mix_then_a_y_loads_distinct_path_ids() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("mix_y.yaml");
    let preset = ChainBlocksPreset {
        id: "mix_y".into(),
        name: Some("Mix Y".into()),
        volume: 100.0,
        instrument: "electric_guitar".into(),
        blocks: vec![
            dual_amp_split("whatever"),
            delay("mid"),
            cab_or_dry_y_split("other"),
        ],
    };
    save_chain_preset_file(&path, &preset).expect("save");
    let loaded = load_chain_preset_file(&path).expect("load");
    let mix = split_of(&loaded.blocks[0]);
    let y = split_of(&loaded.blocks[2]);
    assert_eq!(mix.end, SplitEnd::Mix);
    assert_eq!(y.end, SplitEnd::Y);
    assert_eq!(mix.paths[0][0].id.0, "preset:mix_y:block:0::p0:0");
    assert_eq!(y.paths[0][0].id.0, "preset:mix_y:block:2::p0:0");
    assert!(y.paths[1].is_empty(), "path B of the Y stays empty");
}

#[test]
fn a_split_free_project_stays_version_1() {
    let yaml = serialize_project(&rig_with(vec![delay("pre")])).expect("serialize");
    assert!(
        yaml.contains("version: 1\n") && !yaml.contains("version: 2"),
        "no split, no bump — older builds keep opening it, got:\n{yaml}"
    );
}

#[test]
fn this_build_reads_version_2_and_refuses_version_3() {
    let v1 = serialize_project(&rig_with(vec![delay("pre")])).expect("serialize");
    parse_project(&v1.replacen("version: 1", "version: 2", 1)).expect("version 2 is readable");
    let err = parse_project(&v1.replacen("version: 1", "version: 3", 1))
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
    assert_eq!(split.paths[0][0].id.0, "preset:dual:block:0::p0:0");
    assert_eq!(split.paths[1][0].id.0, "preset:dual:block:0::p1:0");
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
    let a_ids: Vec<&str> = split.paths[0].iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(
        a_ids,
        vec!["preset:partial:block:0::p0:0"],
        "only the block this machine cannot load is dropped"
    );
    assert_eq!(split.paths[1].len(), 1, "path B is untouched");
}

/// A project saved before spec §11 holds its split as `a:` / `b:` lists with
/// `_a` / `_b` knob keys (the owner's DIGITAL rig, 2026-10-01). It still opens:
/// the lists become paths 0 and 1 and every knob keeps its value under the
/// per-path key.
#[test]
fn a_project_saved_with_split_paths_a_and_b_still_opens() {
    let one = serialize_project(&rig_with(vec![delay("da")])).expect("serialize");
    let start = one.find("      - id: da").expect("the delay block");
    let tail = &one[start..];
    let end = tail
        .find("      scene-params")
        .expect("the block list ends");
    let block = |id: &str| -> String {
        tail[..end]
            .replace("- id: da", &format!("- id: {id}"))
            .lines()
            .map(|l| format!("    {l}\n"))
            .collect()
    };
    let split = format!(
        "      - id: sp
        enabled: true
        kind: !Split
          end: mix
          params:
            values:
              balance_a: 0.0
              balance_b: 0.0
              level_to_a: 100.0
              level_to_b: 80.0
              mix_b_polarity: normal
              mix_level_a: 100.0
              mix_level_b: 90.0
              mix_master: 50.0
              mix_master_sum: true
              mix_pan_a: -50.0
              mix_pan_b: 50.0
              split_mode: same
          a:
{}          b:
{}",
        block("pa"),
        block("pb")
    );
    let old = one
        .replacen("version: 1", "version: 2", 1)
        .replacen(&tail[..end], &split, 1);
    let rig = parse_project(&old).expect("a project with an a/b split opens");
    let blocks = &rig.presets["p"].blocks;
    let s = split_of(&blocks[0]);
    let ids: Vec<Vec<&str>> = s
        .paths
        .iter()
        .map(|p| p.iter().map(|b| b.id.0.as_str()).collect())
        .collect();
    assert_eq!(ids, vec![vec!["pa"], vec!["pb"]]);
    let f = |k: &str| s.params.get(k).cloned();
    assert_eq!(f("level_to_1"), Some(ParameterValue::Float(80.0)));
    assert_eq!(f("mix_level_1"), Some(ParameterValue::Float(90.0)));
    assert_eq!(f("mix_pan_0"), Some(ParameterValue::Float(-50.0)));
    assert_eq!(f("mix_pan_1"), Some(ParameterValue::Float(50.0)));
    assert!(
        f("mix_polarity_1").is_some(),
        "path B's polarity carries over"
    );
    for old_key in ["level_to_a", "balance_b", "mix_pan_a", "mix_b_polarity"] {
        assert_eq!(f(old_key), None, "{old_key} is renamed, not kept");
    }
}
