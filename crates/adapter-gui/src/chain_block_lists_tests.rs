//! #328 — an index inside a split path never reads the top level.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, mix_then_y_chain};
use domain::ids::BlockId;
use project::block::{PathRef, PathSide};

fn path(side: PathSide) -> PathRef {
    PathRef {
        split: BlockId("sp".into()),
        side,
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
    assert_eq!(ids(list_at(&c, Some(&path(PathSide::A)))), vec!["a1", "a2"]);
    assert_eq!(ids(list_at(&c, Some(&path(PathSide::B)))), vec!["b1"]);
}

#[test]
fn a_path_to_another_split_or_to_a_chain_without_one_is_nothing() {
    let other = PathRef {
        split: BlockId("gone".into()),
        side: PathSide::A,
    };
    assert!(list_at(&mix_chain(), Some(&other)).is_none());
    assert!(list_at(&chain(vec![core("x")]), Some(&path(PathSide::A))).is_none());
}

#[test]
fn block_zero_of_path_a_is_not_block_zero_of_the_chain() {
    let c = mix_chain();
    assert_eq!(
        block_at(&c, 0, Some(&path(PathSide::A))).unwrap().id.0,
        "a1"
    );
    assert_eq!(block_at(&c, 0, None).unwrap().id.0, "pre");
}

#[test]
fn an_insert_position_is_clamped_to_its_own_list() {
    let c = mix_chain();
    assert_eq!(insert_index(&c, 99, Some(&path(PathSide::B))), Some(1));
    assert_eq!(insert_index(&c, 99, None), Some(3));
}

#[test]
fn a_side_round_trips_through_its_index() {
    assert_eq!(side_index(&PathSide::A), 0);
    assert_eq!(side_index(&PathSide::B), 1);
    assert_eq!(side_from_index(0), Some(PathSide::A));
    assert_eq!(side_from_index(1), Some(PathSide::B));
    assert_eq!(side_from_index(2), None);
}

#[test]
fn mix_then_y_a_path_of_the_y_names_the_ys_lane() {
    let c = mix_then_y_chain();
    let y_path = |side| PathRef {
        split: BlockId("y".into()),
        side,
    };
    assert_eq!(ids(list_at(&c, Some(&y_path(PathSide::A)))), vec!["ya"]);
    assert_eq!(ids(list_at(&c, Some(&y_path(PathSide::B)))), vec!["yb"]);
}
