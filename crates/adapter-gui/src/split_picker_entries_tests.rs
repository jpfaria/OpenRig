//! #328 — Review Focus 5: a split is offered only where it can go.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, mix_then_y_chain, y_chain};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide, SplitEnd};

#[test]
fn a_chain_without_a_split_offers_both_at_its_end() {
    let c = chain(vec![core("a"), core("b")]);
    assert_eq!(
        split_picker_ends(&c, 2, None),
        vec![SplitEnd::Mix, SplitEnd::Y]
    );
}

#[test]
fn y_is_offered_only_at_the_end_of_the_chain() {
    let c = chain(vec![core("a"), core("b")]);
    assert_eq!(split_picker_ends(&c, 1, None), vec![SplitEnd::Mix]);
}

#[test]
fn mix_then_y_a_mix_chain_offers_a_y_only_after_its_mix_at_the_end() {
    // mix_chain(): [pre, sp (Mix), post]
    let c = mix_chain();
    assert_eq!(split_picker_ends(&c, 3, None), vec![SplitEnd::Y]);
    assert!(
        split_picker_ends(&c, 2, None).is_empty(),
        "post would follow the Y"
    );
    assert!(
        split_picker_ends(&c, 1, None).is_empty(),
        "a Y before the Mix, and a second Mix"
    );
}

#[test]
fn mix_then_y_a_y_chain_offers_a_mix_only_before_its_y() {
    // y_chain(): [pre, sp (Y)]
    let c = y_chain();
    assert_eq!(split_picker_ends(&c, 0, None), vec![SplitEnd::Mix]);
    assert_eq!(split_picker_ends(&c, 1, None), vec![SplitEnd::Mix]);
    assert!(
        split_picker_ends(&c, 2, None).is_empty(),
        "nothing goes after the Y"
    );
}

#[test]
fn mix_then_y_a_chain_with_both_offers_no_split() {
    let c = mix_then_y_chain();
    for position in 0..=c.blocks.len() {
        assert!(
            split_picker_ends(&c, position, None).is_empty(),
            "position {position}"
        );
    }
}

#[test]
fn no_split_entry_inside_a_path() {
    let path = PathRef {
        split: BlockId("sp".into()),
        side: PathSide::A,
    };
    assert!(split_picker_ends(&chain(vec![]), 0, Some(&path)).is_empty());
}

#[test]
fn the_entries_sit_after_the_block_types() {
    let ends = [SplitEnd::Mix, SplitEnd::Y];
    assert_eq!(split_end_for_pick(4, 5, &ends), None, "a block type");
    assert_eq!(split_end_for_pick(5, 5, &ends), Some(SplitEnd::Mix));
    assert_eq!(split_end_for_pick(6, 5, &ends), Some(SplitEnd::Y));
    assert_eq!(split_end_for_pick(7, 5, &ends), None);
}

#[test]
fn each_entry_reads_as_a_split() {
    let items = split_picker_items(&[SplitEnd::Mix, SplitEnd::Y]);
    let labels: Vec<String> = items.iter().map(|i| i.label.to_string()).collect();
    assert_eq!(
        labels,
        vec![
            rust_i18n::t!("picker-split-mix").to_string(),
            rust_i18n::t!("picker-split-y").to_string()
        ]
    );
    assert!(items
        .iter()
        .all(|i| i.icon_kind.as_str() == "split" && !i.uses_model_catalog));
}

/// The picker card draws its label in the locale's display font. Bebas Neue
/// (every Latin locale) carries ASCII only: a "→" made the whole label
/// vanish and the entry showed as a bare gear.
#[test]
fn the_entry_labels_render_in_the_latin_display_font() {
    for locale in ["en-US", "pt-BR", "es-ES", "fr-FR", "de-DE"] {
        for key in ["picker-split-mix", "picker-split-y"] {
            let label = rust_i18n::t!(key, locale = locale);
            assert!(
                label.chars().all(|c| c.is_ascii()),
                "{locale} {key}: {label:?} has a glyph Bebas Neue lacks"
            );
        }
    }
}
