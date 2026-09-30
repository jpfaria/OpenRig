//! Tests for `graph_view_model::drop_target` (#328 §5.1): which insert
//! anchor a dragged block lands on.

use super::{
    insert_anchors, linear_chain_layout, resolve_drop_anchor, AnchorSlot, BlockBlueprint,
    ChainStage, GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};

/// in → split → [a1] ∥ [b1] → mixer → out on the default grid:
/// in (80,200) · split (240,200) · a1 (400,140) · b1 (400,260) ·
/// mixer (560,200) · out (720,200). Wire midpoints: in→split (160,200),
/// split→a1 (320,170), split→b1 (320,230), a1→mixer (480,170),
/// b1→mixer (480,230), mixer→out (640,200).
fn resolve(dragged: &str, x: f32, y: f32) -> Option<AnchorSlot> {
    let block = |id: &str| BlockBlueprint::new(id, id.to_uppercase(), NodeCategory::Amp);
    let stages = [
        ChainStage::Single(
            BlockBlueprint::new("in", "In 1", NodeCategory::Input).with_kind(NodeKind::IoInput),
        ),
        ChainStage::Parallel {
            lanes: vec![vec![block("a1")], vec![block("b1")]],
            end: ParallelEnd::Merge,
        },
        ChainStage::Single(
            BlockBlueprint::new("out", "Out 1", NodeCategory::Output).with_kind(NodeKind::IoOutput),
        ),
    ];
    let (nodes, _) = linear_chain_layout(&stages, GridMetrics::default());
    let anchors = insert_anchors(&stages, &nodes);
    resolve_drop_anchor(&nodes, &anchors, dragged, x, y, GridMetrics::default()).map(|a| a.slot)
}

fn lane(lane: usize, index: usize) -> AnchorSlot {
    AnchorSlot::Lane {
        stage: 1,
        lane,
        index,
    }
}

#[test]
fn a_drop_on_an_anchor_resolves_to_it() {
    assert_eq!(
        resolve("a1", 160.0, 200.0),
        Some(AnchorSlot::Stage { index: 1 })
    );
}

#[test]
fn a_drop_into_the_other_lane_resolves_to_that_lanes_slot() {
    // a1 (path A) dropped near the wire split → b1: first in path B.
    assert_eq!(resolve("a1", 322.0, 226.0), Some(lane(1, 0)));
}

#[test]
fn a_drop_on_the_blocks_own_wire_resolves_to_nothing() {
    // a1 → mixer is a1's own wire: landing there leaves the chain
    // unchanged, even though path B's end anchor (480, 230) is only 60px
    // away and must NOT catch the drop.
    assert_eq!(resolve("a1", 480.0, 170.0), None);
    assert_eq!(resolve("a1", 320.0, 170.0), None);
}

#[test]
fn only_a_block_can_be_dropped() {
    assert_eq!(
        resolve("__split_1", 480.0, 230.0),
        None,
        "the split node does not move"
    );
    assert_eq!(
        resolve("in", 480.0, 230.0),
        None,
        "an I/O node does not move"
    );
}

#[test]
fn a_drop_far_from_every_anchor_resolves_to_nothing() {
    assert_eq!(resolve("a1", 400.0, 400.0), None);
}

#[test]
fn an_unknown_block_resolves_to_nothing() {
    assert_eq!(resolve("ghost", 160.0, 200.0), None);
}
