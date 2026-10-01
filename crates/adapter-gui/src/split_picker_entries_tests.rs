//! #328 — Review Focus 5 / spec §11: a split is offered at every "+", in any
//! list; the only rule is that a Y ends the list it sits in.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, mix_then_y_chain, split, y_chain};
use domain::ids::BlockId;
use project::block::{PathRef, SplitEnd};

fn path_of(split: &str, path: usize) -> PathRef {
    PathRef {
        split: BlockId(split.into()),
        path,
    }
}

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
fn a_mix_chain_offers_another_mix_anywhere_and_a_y_at_its_end() {
    // mix_chain(): [pre, sp (Mix), post]
    let c = mix_chain();
    assert_eq!(
        split_picker_ends(&c, 3, None),
        vec![SplitEnd::Mix, SplitEnd::Y]
    );
    assert_eq!(
        split_picker_ends(&c, 2, None),
        vec![SplitEnd::Mix],
        "post would follow a Y"
    );
    assert_eq!(
        split_picker_ends(&c, 1, None),
        vec![SplitEnd::Mix],
        "#328 §11: no limit on the number of splits"
    );
}

#[test]
fn a_y_chain_offers_a_mix_only_before_its_y() {
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
fn a_chain_with_both_offers_a_mix_before_its_y_and_nothing_after() {
    // mix_then_y_chain(): [pre, mx (Mix), mid, y (Y)]
    let c = mix_then_y_chain();
    for position in 0..c.blocks.len() {
        assert_eq!(
            split_picker_ends(&c, position, None),
            vec![SplitEnd::Mix],
            "position {position}"
        );
    }
    assert!(split_picker_ends(&c, c.blocks.len(), None).is_empty());
}

/// Live DIGITAL (2026-10-01): the chain is a single Mix split, nothing after
/// it. The "+" past the mixer must still offer the Y — that is the whole
/// point of "sum two amps, then split to FRFR and to the SYN-5050".
#[test]
fn mix_then_y_a_lone_mix_offers_a_y_after_it() {
    let c = chain(vec![split(
        "sp",
        SplitEnd::Mix,
        vec![core("amp_a")],
        vec![core("amp_b")],
    )]);
    assert_eq!(
        split_picker_ends(&c, 1, None),
        vec![SplitEnd::Mix, SplitEnd::Y]
    );
}

#[test]
fn a_path_offers_a_split_and_a_y_only_at_its_end() {
    // mix_chain(): sp path A = [a1, a2]
    let c = mix_chain();
    let a = path_of("sp", 0);
    assert_eq!(split_picker_ends(&c, 0, Some(&a)), vec![SplitEnd::Mix]);
    assert_eq!(
        split_picker_ends(&c, 2, Some(&a)),
        vec![SplitEnd::Mix, SplitEnd::Y],
        "#328 §11: a Y may end a path, nested at any depth"
    );
}

#[test]
fn a_path_that_does_not_exist_offers_nothing() {
    assert!(split_picker_ends(&chain(vec![]), 0, Some(&path_of("sp", 0))).is_empty());
    assert!(split_picker_ends(&mix_chain(), 0, Some(&path_of("sp", 2))).is_empty());
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
