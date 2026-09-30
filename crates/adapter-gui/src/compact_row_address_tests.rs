//! #328 — a compact row index names one block of the chain, paths included,
//! and an insert slot between two rows names the list the new block joins.

use project::block::{PathRef, PathSide};

use crate::chain_graph_fixtures_tests::{mix_chain, y_chain};
use crate::compact_row_address::{insert_slot, row_block, RowAddress};

fn on(side: PathSide, index: usize) -> RowAddress {
    RowAddress {
        path: Some(PathRef {
            split: domain::ids::BlockId("sp".into()),
            side,
        }),
        index,
    }
}

fn top(index: usize) -> RowAddress {
    RowAddress { path: None, index }
}

#[test]
fn a_row_index_names_the_block_on_that_row() {
    let chain = mix_chain();
    let ids: Vec<String> = (0..6)
        .map(|row| row_block(&chain, row).expect("a row").id.0.clone())
        .collect();
    assert_eq!(ids, vec!["pre", "sp", "a1", "a2", "b1", "post"]);
    assert!(row_block(&chain, 6).is_none());
}

#[test]
fn a_row_address_says_which_list_the_block_is_in() {
    let chain = mix_chain();
    let addresses: Vec<RowAddress> = (0..6)
        .map(|row| crate::compact_row_address::row_address(&chain, row).expect("a row"))
        .collect();
    assert_eq!(
        addresses,
        vec![
            top(0),
            top(1),
            on(PathSide::A, 0),
            on(PathSide::A, 1),
            on(PathSide::B, 0),
            top(2),
        ]
    );
}

#[test]
fn an_insert_slot_adds_to_the_list_of_the_row_above_it() {
    let chain = mix_chain();
    assert_eq!(insert_slot(&chain, 0), top(0), "above the first row");
    assert_eq!(insert_slot(&chain, 1), top(1), "after pre");
    assert_eq!(
        insert_slot(&chain, 2),
        on(PathSide::A, 0),
        "right under the split row: the head of path A"
    );
    assert_eq!(insert_slot(&chain, 4), on(PathSide::A, 2), "after a2");
    assert_eq!(insert_slot(&chain, 5), on(PathSide::B, 1), "after b1");
    assert_eq!(insert_slot(&chain, 6), top(3), "after post");
}

#[test]
fn under_a_y_split_with_empty_paths_the_slot_opens_path_a() {
    let mut chain = y_chain();
    if let project::block::AudioBlockKind::Split(split) = &mut chain.blocks[1].kind {
        split.a.clear();
        split.b.clear();
    }
    assert_eq!(insert_slot(&chain, 2), on(PathSide::A, 0));
}
