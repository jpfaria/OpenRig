//! #328 — an index inside a split path never reads the top level.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, mix_then_y_chain, split_paths};
use domain::ids::BlockId;
use project::block::{PathRef, SplitEnd};

fn path(path: usize) -> PathRef {
    PathRef {
        split: BlockId("sp".into()),
        path,
    }
}

fn ids(list: Option<&[project::block::AudioBlock]>) -> Vec<String> {
    list.unwrap_or_default()
        .iter()
        .map(|b| b.id.0.clone())
        .collect()
}

#[test]
fn the_top_level_is_the_list_without_a_path() {
    assert_eq!(ids(list_at(&mix_chain(), None)), vec!["pre", "sp", "post"]);
}

#[test]
fn a_path_names_the_split_lane() {
    let c = mix_chain();
    assert_eq!(ids(list_at(&c, Some(&path(0)))), vec!["a1", "a2"]);
    assert_eq!(ids(list_at(&c, Some(&path(1)))), vec!["b1"]);
    assert!(list_at(&c, Some(&path(2))).is_none(), "no path C");
}

#[test]
fn a_path_to_another_split_or_to_a_chain_without_one_is_nothing() {
    let other = PathRef {
        split: BlockId("gone".into()),
        path: 0,
    };
    assert!(list_at(&mix_chain(), Some(&other)).is_none());
    assert!(list_at(&chain(vec![core("x")]), Some(&path(0))).is_none());
}

#[test]
fn block_zero_of_path_a_is_not_block_zero_of_the_chain() {
    let c = mix_chain();
    assert_eq!(block_at(&c, 0, Some(&path(0))).unwrap().id.0, "a1");
    assert_eq!(block_at(&c, 0, None).unwrap().id.0, "pre");
}

#[test]
fn an_insert_position_is_clamped_to_its_own_list() {
    let c = mix_chain();
    assert_eq!(insert_index(&c, 99, Some(&path(1))), Some(1));
    assert_eq!(insert_index(&c, 99, None), Some(3));
}

#[test]
fn a_path_round_trips_through_its_index() {
    assert_eq!(path_index(&path(0)), 0);
    assert_eq!(path_index(&path(4)), 4);
    assert_eq!(path_from_index(0), Some(0));
    assert_eq!(path_from_index(4), Some(4));
    assert_eq!(path_from_index(-1), None);
}

#[test]
fn mix_then_y_a_path_of_the_y_names_the_ys_lane() {
    let c = mix_then_y_chain();
    let y_path = |path| PathRef {
        split: BlockId("y".into()),
        path,
    };
    assert_eq!(ids(list_at(&c, Some(&y_path(0)))), vec!["ya"]);
    assert_eq!(ids(list_at(&c, Some(&y_path(1)))), vec!["yb"]);
}

#[test]
fn a_split_nested_in_a_path_is_found_by_its_id() {
    let inner = split_paths(
        "inner",
        SplitEnd::Mix,
        vec![vec![core("i1")], vec![], vec![core("i3")]],
    );
    let c = chain(vec![split_paths(
        "outer",
        SplitEnd::Y,
        vec![vec![core("o1"), inner], vec![]],
    )]);
    let (index, split) = split_by_id(&c, &BlockId("inner".into())).expect("nested split");
    assert_eq!(index, 1, "its index in the path that holds it");
    assert_eq!(split.paths.len(), 3);
    let lane = PathRef {
        split: BlockId("inner".into()),
        path: 2,
    };
    assert_eq!(ids(list_at(&c, Some(&lane))), vec!["i3"]);
}
