//! #913 — the preset-bank projections the compact row and the save dialog read.
//!
//! Every answer here is derived from the rig document alone: which presets a
//! chain's bank holds, which one it is playing, what the save dialog proposes
//! as a filename, and what the load picker's search keeps. The chain id carries
//! the `rig:` prefix, so a legacy (non-rig) chain must fall through empty
//! instead of guessing an input name.

use super::{
    active_preset_id, chain_preset_bank, default_preset_filename_slug, filter_preset_names,
    preset_overwrite_required, preset_rename_target_from_path, strip_io_blocks,
};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::rig::{RigInput, RigPreset, RigProject};
use std::collections::BTreeMap;

fn input(bank: &[(usize, &str)], active: usize) -> RigInput {
    RigInput {
        label: None,
        bank: bank.iter().map(|(i, n)| (*i, (*n).to_string())).collect(),
        active_preset: active,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: Vec::new(),
        loopers: Vec::new(),
    }
}

fn preset(id: &str, name: Option<&str>) -> RigPreset {
    RigPreset {
        id: id.to_string(),
        name: name.map(str::to_string),
        blocks: Vec::new(),
        scene_params: Vec::new(),
        scenes: BTreeMap::new(),
        volume: 100.0,
    }
}

/// One input whose bank has gaps, one preset named, one left to the slug.
fn rig() -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([(
            "guitar".to_string(),
            input(&[(3, "studio-clean"), (1, "lead-boost")], 3),
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([
            ("studio-clean".to_string(), preset("studio-clean", None)),
            (
                "lead-boost".to_string(),
                preset("lead-boost", Some("Lead Boost!")),
            ),
        ]),
        midi: None,
        chain_order: Vec::new(),
    }
}

fn rig_chain() -> ChainId {
    ChainId("rig:guitar".into())
}

#[test]
fn a_preset_arrives_with_io_blocks_the_dispatcher_owns() {
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
    assert_eq!(
        kept.iter().map(|b| b.id.0.as_str()).collect::<Vec<_>>(),
        vec!["gain"],
        "#518: the chain keeps its own I/O across a preset swap"
    );
}

#[test]
fn the_active_preset_id_is_the_bank_entry_the_active_slot_points_at() {
    assert_eq!(
        active_preset_id(&rig_chain(), &rig()).as_deref(),
        Some("studio-clean")
    );
}

#[test]
fn a_slot_with_no_bank_entry_is_playing_no_preset() {
    let mut rig = rig();
    rig.inputs.get_mut("guitar").unwrap().active_preset = 7;
    assert_eq!(active_preset_id(&rig_chain(), &rig), None);
}

#[test]
fn a_non_rig_chain_has_no_active_preset() {
    assert_eq!(active_preset_id(&ChainId("chain:legacy".into()), &rig()), None);
}

#[test]
fn the_bank_is_listed_in_slot_order_with_the_pool_name_when_it_has_one() {
    assert_eq!(
        chain_preset_bank(&rig_chain(), &rig()),
        vec![
            ("lead-boost".to_string(), "Lead Boost!".to_string()),
            ("studio-clean".to_string(), "Studio Clean".to_string()),
        ],
        "slot 1 before slot 3; an unnamed preset falls back to the humanized id"
    );
}

#[test]
fn a_bank_entry_missing_from_the_pool_still_lists_under_its_id() {
    let mut rig = rig();
    rig.presets.remove("studio-clean");
    let bank = chain_preset_bank(&rig_chain(), &rig);
    assert!(bank
        .iter()
        .any(|(id, label)| id == "studio-clean" && label == "Studio Clean"));
}

#[test]
fn a_non_rig_chain_has_an_empty_bank() {
    assert!(chain_preset_bank(&ChainId("chain:legacy".into()), &rig()).is_empty());
    assert!(chain_preset_bank(&ChainId("rig:absent".into()), &rig()).is_empty());
}

#[test]
fn the_save_dialog_proposes_the_active_presets_name_verbatim() {
    let mut rig = rig();
    rig.inputs.get_mut("guitar").unwrap().active_preset = 1;
    assert_eq!(
        default_preset_filename_slug(&rig_chain(), &rig).as_deref(),
        Some("Lead Boost!"),
        "#510: the user's own punctuation survives"
    );
}

#[test]
fn an_unnamed_active_preset_falls_back_to_its_humanized_id() {
    assert_eq!(
        default_preset_filename_slug(&rig_chain(), &rig()).as_deref(),
        Some("Studio Clean")
    );
}

#[test]
fn a_non_rig_chain_proposes_no_filename() {
    assert_eq!(
        default_preset_filename_slug(&ChainId("chain:legacy".into()), &rig()),
        None
    );
}

#[test]
fn a_loaded_file_renames_the_preset_to_its_stem_untouched() {
    assert_eq!(
        preset_rename_target_from_path(std::path::Path::new("/p/my_lead-tone.openrig-preset"))
            .as_deref(),
        Some("my_lead-tone"),
        "#510: dashes and underscores are the user's choice, not ours to humanize"
    );
}

#[test]
fn a_path_with_no_stem_renames_nothing() {
    assert_eq!(preset_rename_target_from_path(std::path::Path::new("/")), None);
    assert_eq!(preset_rename_target_from_path(std::path::Path::new("")), None);
}

#[test]
fn the_picker_search_is_case_insensitive_and_empty_passes_everything() {
    let names = vec![
        "Studio Clean".to_string(),
        "Lead Boost".to_string(),
        "lead rhythm".to_string(),
    ];
    assert_eq!(filter_preset_names(&names, "LEAD").len(), 2);
    assert_eq!(filter_preset_names(&names, "   ").len(), 3);
    assert!(filter_preset_names(&names, "bass").is_empty());
}

#[test]
fn saving_over_an_existing_file_asks_for_confirmation() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(
        !preset_overwrite_required(dir.path(), "Lead Boost"),
        "nothing saved yet"
    );
    let path = super::preset_save_path(dir.path(), "Lead Boost");
    std::fs::write(&path, b"preset:\n").expect("write");
    assert!(preset_overwrite_required(dir.path(), "Lead Boost"));
}
