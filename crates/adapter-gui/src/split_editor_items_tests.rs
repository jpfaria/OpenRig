//! #328 (spec §1.2) — the split editor and the mixer editor show their knobs.

use super::*;
use project::block::split_param_keys::{balance, level_to, mix_level, mix_pan, mix_polarity};
use project::block::split_params::{MIX_MASTER, MIX_MASTER_SUM, SPLIT_MODE};
use project::block::{SplitBlock, SplitEnd};

fn split_block() -> SplitBlock {
    SplitBlock::with_paths(SplitEnd::Mix, vec![vec![], vec![]])
}

fn three_path_block() -> SplitBlock {
    SplitBlock::with_paths(SplitEnd::Mix, vec![vec![], vec![], vec![]])
}

fn paths(kind: SplitEditorKind) -> Vec<String> {
    let mut p: Vec<String> = split_editor_items(&split_block(), kind)
        .iter()
        .map(|i| i.path.to_string())
        .collect();
    p.sort();
    p
}

fn sorted(keys: &[String]) -> Vec<String> {
    let mut k = keys.to_vec();
    k.sort();
    k
}

#[test]
fn the_split_editor_shows_mode_levels_and_balances() {
    assert_eq!(
        paths(SplitEditorKind::Split),
        sorted(&[
            SPLIT_MODE.into(),
            level_to(0),
            level_to(1),
            balance(0),
            balance(1)
        ])
    );
}

#[test]
fn the_mixer_editor_shows_levels_pans_polarity_and_master() {
    assert_eq!(
        paths(SplitEditorKind::Mixer),
        sorted(&[
            mix_level(0),
            mix_level(1),
            mix_pan(0),
            mix_pan(1),
            mix_polarity(0),
            mix_polarity(1),
            MIX_MASTER.into(),
            MIX_MASTER_SUM.into()
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
    assert_eq!(kind(&mix_polarity(1)), "enum");
}

#[test]
fn an_option_index_names_its_value() {
    assert_eq!(
        option_value(SplitEditorKind::Split, SPLIT_MODE, 1).as_deref(),
        Some("dual_mono")
    );
    assert_eq!(
        option_value(SplitEditorKind::Mixer, &mix_polarity(1), 1).as_deref(),
        Some("invert")
    );
    assert_eq!(
        option_value(SplitEditorKind::Mixer, &mix_pan(0), 0),
        None,
        "not a choice"
    );
}

#[test]
fn a_third_path_brings_its_own_knobs() {
    let rows = split_editor_items(&three_path_block(), SplitEditorKind::Mixer);
    assert!(rows.iter().any(|r| r.path.as_str() == mix_pan(2)));
    assert_eq!(
        option_value(SplitEditorKind::Mixer, &mix_polarity(2), 1).as_deref(),
        Some("invert"),
        "#328 §11.2: the choices of path C are named like those of A and B"
    );
}

#[test]
fn the_overlay_kind_index_maps_both_ways() {
    assert_eq!(SplitEditorKind::from_index(0), Some(SplitEditorKind::Split));
    assert_eq!(SplitEditorKind::from_index(1), Some(SplitEditorKind::Mixer));
    assert_eq!(SplitEditorKind::from_index(2), None);
}
