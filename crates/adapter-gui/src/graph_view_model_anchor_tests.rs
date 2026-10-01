//! Tests for `graph_view_model::anchors` (#328 §5.1): one insert anchor
//! per wire, at its midpoint, carrying the slot a block lands in.

use super::{
    insert_anchors, linear_chain_layout, AnchorSlot, BlockBlueprint, ChainStage, GraphAnchor,
    GridMetrics, NodeCategory, NodeKind, ParallelEnd,
};
use domain::ids::BlockId;
use project::block::PathRef;

/// The slot `index` of the top-level list.
fn top(index: usize) -> AnchorSlot {
    AnchorSlot { path: None, index }
}

/// The slot `index` of path `lane` of the split "sp".
fn lane(lane: usize, index: usize) -> AnchorSlot {
    AnchorSlot {
        path: Some(PathRef {
            split: BlockId("sp".into()),
            path: lane,
        }),
        index,
    }
}

fn block(id: &str) -> BlockBlueprint {
    BlockBlueprint::new(id, id.to_uppercase(), NodeCategory::Drive)
}

fn io_in(id: &str) -> BlockBlueprint {
    BlockBlueprint::new(id, "In 1", NodeCategory::Input).with_kind(NodeKind::IoInput)
}

fn io_out(id: &str) -> BlockBlueprint {
    BlockBlueprint::new(id, "Out 1", NodeCategory::Output).with_kind(NodeKind::IoOutput)
}

fn anchors_of(stages: &[ChainStage]) -> Vec<GraphAnchor> {
    let (nodes, _) = linear_chain_layout(stages, GridMetrics::default());
    insert_anchors(stages, &nodes)
}

fn slots(anchors: &[GraphAnchor]) -> Vec<(&str, &str, AnchorSlot)> {
    anchors
        .iter()
        .map(|a| (a.from_id.as_str(), a.to_id.as_str(), a.slot.clone()))
        .collect()
}

/// in → split → [a1, a2] ∥ [b1] → mixer → out
fn split_mix() -> Vec<ChainStage> {
    vec![
        ChainStage::Single(io_in("in")),
        ChainStage::Parallel {
            split_id: "sp".into(),
            lanes: vec![
                vec![
                    ChainStage::Single(block("a1")),
                    ChainStage::Single(block("a2")),
                ],
                vec![ChainStage::Single(block("b1"))],
            ],
            end: ParallelEnd::Merge,
        },
        ChainStage::Single(io_out("out")),
    ]
}

/// in → split → [a1 → out_a] ∥ [out_b]
fn split_y() -> Vec<ChainStage> {
    vec![
        ChainStage::Single(io_in("in")),
        ChainStage::Parallel {
            split_id: "sp".into(),
            lanes: vec![
                vec![
                    ChainStage::Single(block("a1")),
                    ChainStage::Single(io_out("out_a")),
                ],
                vec![ChainStage::Single(io_out("out_b"))],
            ],
            end: ParallelEnd::Fan,
        },
    ]
}

#[test]
fn every_wire_gets_exactly_one_anchor() {
    for stages in [split_mix(), split_y()] {
        let (nodes, edges) = linear_chain_layout(&stages, GridMetrics::default());
        let anchors = insert_anchors(&stages, &nodes);
        let mut wires: Vec<(String, String)> = edges
            .iter()
            .map(|e| (e.from_id.clone(), e.to_id.clone()))
            .collect();
        let mut anchored: Vec<(String, String)> = anchors
            .iter()
            .map(|a| (a.from_id.clone(), a.to_id.clone()))
            .collect();
        wires.sort();
        anchored.sort();
        assert_eq!(
            anchored, wires,
            "anchors drifted from the wires the chain builder draws"
        );
        let mut ids: Vec<&str> = anchors.iter().map(|a| a.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), anchors.len(), "anchor ids must be unique");
    }
}

#[test]
fn a_linear_chain_anchors_before_each_stage() {
    let stages = [
        ChainStage::Single(io_in("in")),
        ChainStage::Single(block("od")),
        ChainStage::Single(block("amp")),
        ChainStage::Single(io_out("out")),
    ];
    assert_eq!(
        slots(&anchors_of(&stages)),
        [
            ("in", "od", top(1)),
            ("od", "amp", top(2)),
            ("amp", "out", top(3)),
        ]
    );
}

#[test]
fn merge_lanes_get_a_slot_per_gap_and_one_at_each_lane_end() {
    assert_eq!(
        slots(&anchors_of(&split_mix())),
        [
            ("in", "__split_sp", top(1)),
            ("__split_sp", "a1", lane(0, 0)),
            ("a1", "a2", lane(0, 1)),
            ("a2", "__merge_sp", lane(0, 2)),
            ("__split_sp", "b1", lane(1, 0)),
            ("b1", "__merge_sp", lane(1, 1)),
            ("__merge_sp", "out", top(2)),
        ]
    );
}

#[test]
fn a_fan_lane_end_anchor_sits_before_its_terminal() {
    assert_eq!(
        slots(&anchors_of(&split_y())),
        [
            ("in", "__split_sp", top(1)),
            ("__split_sp", "a1", lane(0, 0)),
            ("a1", "out_a", lane(0, 1)),
            ("__split_sp", "out_b", lane(1, 0)),
        ],
        "nothing is anchored after a terminal"
    );
}

#[test]
fn an_anchor_sits_on_its_wire_midpoint() {
    let anchors = anchors_of(&split_mix());
    let split_to_b1 = anchors
        .iter()
        .find(|a| a.from_id == "__split_sp" && a.to_id == "b1")
        .expect("split → b1 anchor");
    // Default grid: split at (240, 200), b1 at (400, 260).
    assert_eq!((split_to_b1.x, split_to_b1.y), (320.0, 230.0));
}

#[test]
fn anchor_ids_name_their_slot() {
    assert_eq!(top(3).anchor_id(), "top:3");
    assert_eq!(lane(0, 2).anchor_id(), "path:sp:0:2");
}

#[test]
fn an_empty_segment_keeps_its_plus_visible() {
    let empty_chain = [
        ChainStage::Single(io_in("in")),
        ChainStage::Single(io_out("out")),
    ];
    assert!(
        anchors_of(&empty_chain).iter().all(|a| a.always_visible),
        "an empty chain shows its +"
    );

    let empty_lane = [
        ChainStage::Single(io_in("in")),
        ChainStage::Parallel {
            split_id: "sp".into(),
            lanes: vec![vec![ChainStage::Single(block("a1"))], vec![]],
            end: ParallelEnd::Merge,
        },
        ChainStage::Single(io_out("out")),
    ];
    let visible: Vec<AnchorSlot> = anchors_of(&empty_lane)
        .iter()
        .filter(|a| a.always_visible)
        .map(|a| a.slot.clone())
        .collect();
    assert!(
        visible.contains(&lane(1, 0)),
        "the empty lane B shows its +: {visible:?}"
    );
    assert!(
        !visible.contains(&lane(0, 0)),
        "a wire into a block hides its + until hover: {visible:?}"
    );

    assert!(
        anchors_of(&split_y())
            .iter()
            .any(|a| a.slot == lane(1, 0) && a.always_visible),
        "an empty Y path (only its output) shows its +"
    );
}
