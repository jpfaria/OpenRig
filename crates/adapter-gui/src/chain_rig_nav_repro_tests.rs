//! #436 user-repro tests for the rig nav projection that don't fit in
//! `chain_rig_nav_tests.rs` without breaking the 600-line Rust cap
//! (one concern per file). Faithful to the real reproject() sequence.

use super::preset_slot_at;
use engine::rig_runtime::{rig_to_legacy_project, switch_and_project_input};
use project::block::InputEntry;
use project::chain::ChainInputMode;
use project::rig::{RigInput, RigPreset, RigProject};
use std::collections::{BTreeMap, BTreeSet};

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    for n in ["clean", "drive", "lead"] {
        presets.insert(n.to_string(), RigPreset::from_legacy_blocks(vec![], 100.0));
    }
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: None,
            sources: vec![InputEntry {
                device_id: domain::ids::DeviceId("d".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank: BTreeMap::from([
                (1, "clean".to_string()),
                (2, "drive".to_string()),
                (3, "lead".to_string()),
            ]),
            active_preset: 2,
            active_scene: 4,
            routing: vec![],
        },
    );
    RigProject {
        name: Some("p".into()),
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
    }
}

// Reproduces the user's report ("alterei o preset para outra config de
// blocos e não altera quando seleciono no select") FAITHFULLY: mirrors
// the exact reproject() sequence — sync_synthetic_into_rig FIRST (the
// step that captures the current synthetic chain before switching),
// THEN position→key→switch_and_project_input — and asserts the
// projected processing blocks actually become the target preset's.
#[test]
fn select_other_preset_via_reproject_sequence_changes_blocks() {
    use domain::ids::BlockId;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let blk = |id: &str| AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    // clean = [A, B] ; drive = [C] — structurally different chains.
    let mut r = rig();
    r.presets.get_mut("clean").unwrap().blocks = vec![blk("A"), blk("B")];
    r.presets.get_mut("drive").unwrap().blocks = vec![blk("C")];
    r.inputs.get_mut("input-1").unwrap().active_preset = 1; // clean
    r.inputs.get_mut("input-1").unwrap().active_scene = 1;

    let core_ids = |c: &project::chain::Chain| {
        c.blocks
            .iter()
            .filter_map(|b| match &b.kind {
                AudioBlockKind::Core(_) => Some(b.id.0.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    // Build the synthetic project exactly like the live session holds it.
    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    assert_eq!(core_ids(&proj.chains[0]), vec!["A", "B"], "starts on clean");

    // ── reproject() step 1: capture current synthetic chain back.
    super::sync_synthetic_into_rig(&mut r, &proj);
    // ── reproject() step 2: user picked PresetSelect row 1 (= "drive").
    let key = preset_slot_at(&r, "input-1", 1).expect("position 1 → key");
    let rebuilt =
        switch_and_project_input(&mut r, "input-1", Some(key), None).expect("rebuilt chain");
    // ── reproject() step 3: swap it into the project (id-aligned).
    proj.chains[0] = rebuilt;

    assert_eq!(
        core_ids(&proj.chains[0]),
        vec!["C"],
        "selecting 'drive' must project drive's blocks ([C]), not clean's"
    );
}

// User invariant: "scene é só para parâmetros e se está ativo ou não um
// bloco". Switching scene must NEVER change the block STRUCTURE (same
// block ids, same count, same order) — only `enabled` and marked params
// may differ. Mirrors the real reproject() sequence (sync first, then
// switch) across scene 1 ↔ 2, including a bypass edit on scene 1.
#[test]
fn switching_scene_never_changes_block_structure() {
    use domain::ids::BlockId;
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::param::ParameterSet;

    let blk = |id: &str| AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    };
    let mut r = rig();
    r.presets.get_mut("clean").unwrap().blocks = vec![blk("A"), blk("B")];
    r.inputs.get_mut("input-1").unwrap().active_preset = 1; // clean
    r.inputs.get_mut("input-1").unwrap().active_scene = 1;
    r.add_scene_to_input("input-1"); // scene 2 exists
    r.inputs.get_mut("input-1").unwrap().active_scene = 1;

    let structure = |c: &project::chain::Chain| {
        c.blocks
            .iter()
            .filter_map(|b| match &b.kind {
                AudioBlockKind::Core(_) => Some(b.id.0.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    let mut proj = rig_to_legacy_project(&r, &BTreeSet::new());
    assert_eq!(structure(&proj.chains[0]), vec!["A", "B"], "scene 1 base");

    // Edit on scene 1: bypass B (enabled=false) on the synthetic chain.
    for c in proj.chains.iter_mut().filter(|c| c.id.0 == "rig:input-1") {
        for b in c.blocks.iter_mut() {
            if b.id.0 == "B" {
                b.enabled = false;
            }
        }
    }

    // reproject() for "switch to scene 2": sync FIRST, then switch.
    super::sync_synthetic_into_rig(&mut r, &proj);
    let c2 = switch_and_project_input(&mut r, "input-1", None, Some(2)).expect("scene 2");
    assert_eq!(
        structure(&c2),
        vec!["A", "B"],
        "scene 2 must keep the SAME blocks (structure is preset-level)"
    );

    // Back to scene 1: structure still identical, B still bypassed.
    proj.chains[0] = c2;
    super::sync_synthetic_into_rig(&mut r, &proj);
    let c1 = switch_and_project_input(&mut r, "input-1", None, Some(1)).expect("scene 1");
    assert_eq!(structure(&c1), vec!["A", "B"], "scene 1 structure intact");
    let b_enabled = c1.blocks.iter().find(|b| b.id.0 == "B").map(|b| b.enabled);
    assert_eq!(
        b_enabled,
        Some(false),
        "scene 1 keeps B bypassed (param/enable only)"
    );
}
