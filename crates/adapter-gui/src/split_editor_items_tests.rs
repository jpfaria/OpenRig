//! #328 (spec §1.2) — the split editor and the mixer editor show their knobs.

use super::*;
use project::block::split_params::{
    default_split_params, BALANCE_A, BALANCE_B, LEVEL_TO_A, LEVEL_TO_B, MIX_B_POLARITY,
    MIX_LEVEL_A, MIX_LEVEL_B, MIX_MASTER, MIX_MASTER_SUM, MIX_PAN_A, MIX_PAN_B, SPLIT_MODE,
};
use project::block::{SplitBlock, SplitEnd};

fn split_block() -> SplitBlock {
    SplitBlock {
        end: SplitEnd::Mix,
        params: default_split_params(),
        a: vec![],
        b: vec![],
    }
}

fn paths(kind: SplitEditorKind) -> Vec<String> {
    let mut p: Vec<String> = split_editor_items(&split_block(), kind)
        .iter()
        .map(|i| i.path.to_string())
        .collect();
    p.sort();
    p
}

fn sorted(keys: &[&str]) -> Vec<String> {
    let mut k: Vec<String> = keys.iter().map(|s| s.to_string()).collect();
    k.sort();
    k
}

#[test]
fn the_split_editor_shows_mode_levels_and_balances() {
    assert_eq!(
        paths(SplitEditorKind::Split),
        sorted(&[SPLIT_MODE, LEVEL_TO_A, LEVEL_TO_B, BALANCE_A, BALANCE_B])
    );
}

#[test]
fn the_mixer_editor_shows_levels_pans_polarity_and_master() {
    assert_eq!(
        paths(SplitEditorKind::Mixer),
        sorted(&[
            MIX_LEVEL_A,
            MIX_LEVEL_B,
            MIX_PAN_A,
            MIX_PAN_B,
            MIX_B_POLARITY,
            MIX_MASTER,
            MIX_MASTER_SUM
        ])
    );
}

#[test]
fn switches_and_choices_get_their_widgets() {
    let rows = split_editor_items(&split_block(), SplitEditorKind::Mixer);
    let kind = |path: &str| {
        rows.iter()
            .find(|r| r.path.as_str() == path)
            .unwrap()
            .widget_kind
            .to_string()
    };
    assert_eq!(kind(MIX_MASTER_SUM), "bool");
    assert_eq!(kind(MIX_B_POLARITY), "enum");
}

#[test]
fn an_option_index_names_its_value() {
    assert_eq!(
        option_value(SplitEditorKind::Split, SPLIT_MODE, 1).as_deref(),
        Some("dual_mono")
    );
    assert_eq!(
        option_value(SplitEditorKind::Mixer, MIX_B_POLARITY, 1).as_deref(),
        Some("invert")
    );
    assert_eq!(
        option_value(SplitEditorKind::Mixer, MIX_PAN_A, 0),
        None,
        "not a choice"
    );
}

#[test]
fn the_overlay_kind_index_maps_both_ways() {
    assert_eq!(SplitEditorKind::from_index(0), Some(SplitEditorKind::Split));
    assert_eq!(SplitEditorKind::from_index(1), Some(SplitEditorKind::Mixer));
    assert_eq!(SplitEditorKind::from_index(2), None);
}
