//! #328 (spec §5.1, §7 "drop-target resolution") — a "+" or a drop names a
//! place in the chain: a position in one list plus the path of that list.

use super::*;
use crate::chain_graph_fixtures_tests::{
    chain, core, mix_chain, mix_then_y_chain, split, y_chain, FIRST_SPLIT_NODE_ID,
};
use crate::chain_graph_ids::INPUT_NODE_ID;
use domain::ids::BlockId;
use project::block::{PathRef, SplitEnd};

// The top level counts the input node as stage 0. mix_chain() lays out as
// 0 input · 1 pre · 2 split(A: a1 a2 | B: b1) · 3 post · 4 output; a path
// counts its own list from 0.
fn stage(index: usize) -> AnchorSlot {
    AnchorSlot { path: None, index }
}

fn path_of(split: &str, path: usize) -> PathRef {
    PathRef {
        split: BlockId(split.into()),
        path,
    }
}

/// The slot `index` of path `path` of the split `split`.
fn lane_of(split: &str, path: usize, index: usize) -> AnchorSlot {
    AnchorSlot {
        path: Some(path_of(split, path)),
        index,
    }
}

/// The slot `index` of path `path` of the split "sp".
fn lane(path: usize, index: usize) -> AnchorSlot {
    lane_of("sp", path, index)
}

fn on(path: usize) -> Option<PathRef> {
    Some(path_of("sp", path))
}

fn at(position: usize, path: Option<PathRef>) -> Option<InsertTarget> {
    Some(InsertTarget { position, path })
}

#[test]
fn an_anchor_id_parses_back_into_its_slot() {
    for slot in [
        stage(0),
        stage(3),
        lane(0, 0),
        lane(1, 4),
        lane(7, 2),
        lane_of("rig:input-4:block:c723fb6e", 2, 1),
    ] {
        assert_eq!(
            parse_anchor(&slot.anchor_id()),
            Some(slot.clone()),
            "{}",
            slot.anchor_id()
        );
    }
    for bad in [
        "",
        "top",
        "top:x",
        "path:sp:0",
        "path::0:0",
        "path:sp:x:0",
        "edge:1",
    ] {
        assert_eq!(parse_anchor(bad), None, "{bad:?}");
    }
}

#[test]
fn a_top_level_anchor_inserts_before_its_stage() {
    let c = mix_chain();
    assert_eq!(insert_target(&c, &stage(1)), at(0, None)); // input → pre
    assert_eq!(insert_target(&c, &stage(2)), at(1, None)); // pre → split
    assert_eq!(insert_target(&c, &stage(3)), at(2, None)); // mixer → post
    assert_eq!(insert_target(&c, &stage(4)), at(3, None)); // post → output
    assert_eq!(
        insert_target(&c, &stage(0)),
        None,
        "nothing goes before the input node"
    );
    assert_eq!(insert_target(&c, &stage(5)), None, "past the output node");
}

#[test]
fn a_lane_anchor_inserts_in_that_path() {
    let c = mix_chain();
    assert_eq!(insert_target(&c, &lane(0, 0)), at(0, on(0))); // split → a1
    assert_eq!(insert_target(&c, &lane(0, 1)), at(1, on(0))); // a1 → a2
    assert_eq!(insert_target(&c, &lane(0, 2)), at(2, on(0))); // a2 → mixer
    assert_eq!(insert_target(&c, &lane(1, 1)), at(1, on(1))); // b1 → mixer
    assert_eq!(
        insert_target(&c, &lane(1, 2)),
        None,
        "past the end of path B"
    );
    assert_eq!(
        insert_target(&c, &lane_of("pre", 0, 0)),
        None,
        "pre is not a split"
    );
    assert_eq!(
        insert_target(&c, &lane(2, 0)),
        None,
        "this split has two paths"
    );
}

#[test]
fn an_empty_lane_is_reached_by_its_own_anchor() {
    // 0 input · 1 split(A: — | B: —) · 2 output: each empty lane has its own
    // split → mixer wire, with its own anchor id.
    let c = chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]);
    assert_eq!(insert_target(&c, &lane(0, 0)), at(0, on(0)));
    assert_eq!(insert_target(&c, &lane(1, 0)), at(0, on(1)));
}

#[test]
fn a_y_lane_ends_before_its_own_output_node() {
    // 0 input · 1 pre · 2 split(A: a1 → out A | B: b1 → out B)
    let c = y_chain();
    assert_eq!(insert_target(&c, &lane(0, 1)), at(1, on(0))); // a1 → out A
    assert_eq!(insert_target(&c, &lane(1, 0)), at(0, on(1))); // split → b1
}

/// Live DIGITAL (2026-10-01): one Mix split and nothing else — 0 input ·
/// 1 split · 2 output. The anchor past the mixer must reach the chain's end,
/// or there is nowhere to put the Y.
#[test]
fn mix_then_y_a_lone_mix_has_an_anchor_past_its_mixer() {
    let c = chain(vec![split(
        "sp",
        SplitEnd::Mix,
        vec![core("amp_a")],
        vec![core("amp_b")],
    )]);
    assert_eq!(insert_target(&c, &stage(2)), at(1, None));
}

#[test]
fn a_lane_anchor_on_a_chain_without_a_split_is_nothing() {
    assert_eq!(insert_target(&chain(vec![core("x")]), &lane(0, 0)), None);
}

fn moved(block: &str, new_position: usize, path: Option<PathRef>) -> Option<MoveTarget> {
    Some(MoveTarget {
        block: BlockId(block.into()),
        new_position,
        path,
    })
}

#[test]
fn dragging_a_block_to_the_other_lane_moves_it_into_that_path() {
    let c = mix_chain();
    assert_eq!(move_target(&c, "a1", &lane(1, 1)), moved("a1", 1, on(1)));
    assert_eq!(move_target(&c, "b1", &lane(0, 0)), moved("b1", 0, on(0)));
}

#[test]
fn a_move_inside_one_list_counts_after_the_block_is_lifted() {
    let c = mix_chain();
    // a1 dropped at the end of lane A (after a2): lifted first, so it lands at 1.
    assert_eq!(move_target(&c, "a1", &lane(0, 2)), moved("a1", 1, on(0)));
    assert_eq!(move_target(&c, "a2", &lane(0, 0)), moved("a2", 0, on(0)));
}

#[test]
fn dropping_a_block_on_its_own_slot_is_no_move() {
    let c = mix_chain();
    assert_eq!(move_target(&c, "a1", &lane(0, 0)), None);
    assert_eq!(move_target(&c, "a1", &lane(0, 1)), None);
}

#[test]
fn shared_and_path_positions_trade_blocks() {
    let c = mix_chain();
    assert_eq!(move_target(&c, "pre", &lane(0, 2)), moved("pre", 2, on(0)));
    assert_eq!(move_target(&c, "b1", &stage(3)), moved("b1", 2, None));
}

#[test]
fn only_block_nodes_move() {
    let c = mix_chain();
    assert_eq!(move_target(&c, FIRST_SPLIT_NODE_ID, &lane(0, 0)), None);
    assert_eq!(move_target(&c, INPUT_NODE_ID, &lane(0, 0)), None);
}

#[test]
fn mix_then_y_a_lane_anchor_names_its_split() {
    // 0 input · 1 pre · 2 Mix mx(A: ma | B: mb) · 3 mid · 4 Y y(A: ya | B: yb)
    let c = mix_then_y_chain();
    let on_y = |path| Some(path_of("y", path));
    let on_mx = |path| Some(path_of("mx", path));
    assert_eq!(insert_target(&c, &lane_of("y", 0, 1)), at(1, on_y(0)));
    assert_eq!(insert_target(&c, &lane_of("y", 1, 0)), at(0, on_y(1)));
    assert_eq!(insert_target(&c, &lane_of("mx", 1, 1)), at(1, on_mx(1)));
    assert_eq!(
        insert_target(&c, &lane_of("mid", 0, 0)),
        None,
        "mid is not a split"
    );
}
