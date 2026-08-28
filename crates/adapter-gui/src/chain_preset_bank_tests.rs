//! #323 phase 2 — what a chain's preset bank answers.
//!
//! Pure: a rig in memory, no window. These pin the three questions the looper
//! drawer and the save dialog ask — which preset is playing, what the bank
//! holds in slot order, and what to call the file — including the answers for
//! a chain that is not projected from a rig at all.

use super::{active_preset_id, chain_preset_bank, default_preset_filename_slug, strip_io_blocks};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::rig::{RigInput, RigPreset, RigProject};
use std::collections::BTreeMap;

fn preset(id: &str, name: Option<&str>) -> RigPreset {
    RigPreset {
        id: id.into(),
        name: name.map(|n| n.to_string()),
        blocks: Vec::new(),
        scene_params: Vec::new(),
        scenes: BTreeMap::new(),
        volume: 100.0,
    }
}

/// One rig input whose bank has gaps (slots 1, 3, 5) with slot 3 active — the
/// shape a real bank takes once presets are added and removed.
fn rig() -> RigProject {
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: Some("Guitarra".into()),
            bank: BTreeMap::from([
                (1, "clean".to_string()),
                (3, "drive".to_string()),
                (5, "lead".to_string()),
            ]),
            active_preset: 3,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
            loopers: Vec::new(),
        },
    );
    RigProject {
        name: Some("Studio".into()),
        inputs,
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([
            ("clean".to_string(), preset("clean", Some("Clean Tone"))),
            ("drive".to_string(), preset("drive", Some("Crunch"))),
            // `lead` is in the bank but has no pool entry with a name.
            ("lead".to_string(), preset("lead", None)),
        ]),
        midi: None,
        chain_order: Vec::new(),
    }
}

fn chain(name: &str) -> ChainId {
    ChainId(name.into())
}

#[test]
fn the_active_preset_is_the_one_the_bank_slot_points_at() {
    assert_eq!(
        active_preset_id(&chain("rig:input-1"), &rig()).as_deref(),
        Some("drive"),
        "slot 3 is active, and slot 3 holds `drive`"
    );
}

#[test]
fn a_non_rig_chain_has_no_active_preset() {
    assert!(active_preset_id(&chain("chain:plain"), &rig()).is_none());
    assert!(active_preset_id(&chain("rig:input-9"), &rig()).is_none());
}

#[test]
fn the_bank_lists_its_presets_in_slot_order_with_display_names() {
    let bank = chain_preset_bank(&chain("rig:input-1"), &rig());

    assert_eq!(
        bank,
        vec![
            ("clean".to_string(), "Clean Tone".to_string()),
            ("drive".to_string(), "Crunch".to_string()),
            ("lead".to_string(), "Lead".to_string()),
        ],
        "ascending slot order, and a preset with no name falls back to a humanized id"
    );
}

#[test]
fn a_non_rig_chain_has_an_empty_bank() {
    assert!(chain_preset_bank(&chain("chain:plain"), &rig()).is_empty());
    assert!(chain_preset_bank(&chain("rig:input-9"), &rig()).is_empty());
}

#[test]
fn the_save_dialog_is_seeded_with_the_active_presets_display_name() {
    assert_eq!(
        default_preset_filename_slug(&chain("rig:input-1"), &rig()).as_deref(),
        Some("Crunch"),
        "the name the user sees, verbatim — not a slug (#510)"
    );
}

#[test]
fn a_non_rig_chain_seeds_no_filename() {
    assert!(default_preset_filename_slug(&chain("chain:plain"), &rig()).is_none());
}

#[test]
fn a_preset_dispatched_onto_a_chain_carries_no_io_blocks() {
    let blocks = vec![
        AudioBlock {
            id: BlockId("in".into()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".into(),
                io: "io-1".into(),
                endpoint: "In 1".into(),
            }),
        },
        AudioBlock {
            id: BlockId("gain".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: Default::default(),
            }),
        },
        AudioBlock {
            id: BlockId("out".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                io: "io-1".into(),
                endpoint: "Out 1".into(),
            }),
        },
    ];

    let kept = strip_io_blocks(blocks);

    assert_eq!(kept.len(), 1, "the dispatcher owns the chain's I/O (#518)");
    assert_eq!(kept[0].id.0, "gain");
}
