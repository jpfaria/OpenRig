//! #328 — Review Focus 5: a split is offered only where it can go.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain};
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
fn no_split_entry_when_the_chain_already_has_one() {
    assert!(split_picker_ends(&mix_chain(), 3, None).is_empty());
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
