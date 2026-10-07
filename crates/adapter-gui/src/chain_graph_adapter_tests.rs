//! #328 (spec §5.1) — left to right: input → shared blocks → split → lanes
//! (A on top) → mixer → shared blocks → output; for Y each lane ends in its
//! own output node.

use super::*;
use crate::chain_graph_fixtures_tests::{
    chain, core, mix_chain, split, y_chain, FIRST_MIXER_NODE_ID, FIRST_SPLIT_NODE_ID,
};
use crate::chain_graph_ids::{leaf_output_node_id, INPUT_NODE_ID, OUTPUT_NODE_ID};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_anchor::{insert_target, parse_anchor};
use crate::graph_view_model::split_node_id;
use crate::graph_view_model::NodeKind;
use domain::ids::BlockId;
use project::block::{PathRef, SplitEnd};

fn leaf(split: &str, path: usize) -> PathRef {
    PathRef {
        split: BlockId(split.into()),
        path,
    }
}

/// The output node of Y leaf `path` of split `split`.
fn leaf_out(split: &str, path: usize) -> String {
    leaf_output_node_id(&leaf(split, path))
}

fn labels() -> IoLabels {
    IoLabels {
        input: "In 1".into(),
        output: "Out".into(),
        leaves: vec![
            (leaf("sp", 0), "Out A".into()),
            (leaf("sp", 1), "Out B".into()),
        ],
        ports: Vec::new(),
    }
}

fn node<'a>(graph: &'a ChainGraph, id: &str) -> &'a crate::graph_view_model::GraphNode {
    graph
        .nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn has_edge(graph: &ChainGraph, from: &str, to: &str) -> bool {
    graph
        .edges
        .iter()
        .any(|e| e.from_id == from && e.to_id == to)
}

#[test]
fn a_linear_chain_is_one_lane_from_input_to_output() {
    let graph = chain_graph(&chain(vec![core("a"), core("b")]), &labels());
    let ids: Vec<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec![INPUT_NODE_ID, "a", "b", OUTPUT_NODE_ID]);
    assert_eq!((graph.lanes, graph.columns), (1, 4));
    for (column, id) in ids.iter().enumerate() {
        let n = node(&graph, id);
        assert_eq!(
            (n.x, n.y),
            (CARD_HALF + column as f32 * COLUMN_SPACING, CARD_HALF)
        );
    }
    assert!(
        has_edge(&graph, INPUT_NODE_ID, "a")
            && has_edge(&graph, "a", "b")
            && has_edge(&graph, "b", OUTPUT_NODE_ID)
    );
    assert_eq!(node(&graph, INPUT_NODE_ID).label, "In 1");
    assert_eq!(node(&graph, OUTPUT_NODE_ID).label, "Out");
}

#[test]
fn a_split_to_mix_chain_runs_two_lanes_between_split_and_mixer() {
    let graph = chain_graph(&mix_chain(), &labels());
    assert_eq!(graph.lanes, 2);
    let split_y = node(&graph, FIRST_SPLIT_NODE_ID).y;
    assert!(
        node(&graph, "a1").y < split_y && split_y < node(&graph, "b1").y,
        "lane A above, lane B below"
    );
    assert_eq!(node(&graph, "pre").y, split_y);
    assert_eq!(node(&graph, "post").y, node(&graph, FIRST_MIXER_NODE_ID).y);
    for (from, to) in [
        (INPUT_NODE_ID, "pre"),
        ("pre", FIRST_SPLIT_NODE_ID),
        (FIRST_SPLIT_NODE_ID, "a1"),
        ("a1", "a2"),
        ("a2", FIRST_MIXER_NODE_ID),
        (FIRST_SPLIT_NODE_ID, "b1"),
        ("b1", FIRST_MIXER_NODE_ID),
        (FIRST_MIXER_NODE_ID, "post"),
        ("post", OUTPUT_NODE_ID),
    ] {
        assert!(has_edge(&graph, from, to), "missing wire {from} → {to}");
    }
    assert!(graph.nodes.iter().all(|n| !n.id.starts_with("__out_")));
}

#[test]
fn a_y_chain_ends_each_lane_in_its_own_output_node() {
    let graph = chain_graph(&y_chain(), &labels());
    assert!(graph
        .nodes
        .iter()
        .all(|n| n.id != OUTPUT_NODE_ID && n.id != FIRST_MIXER_NODE_ID));
    assert_eq!(node(&graph, &leaf_out("sp", 0)).y, node(&graph, "a1").y);
    assert_eq!(node(&graph, &leaf_out("sp", 1)).y, node(&graph, "b1").y);
    assert!(
        has_edge(&graph, "a1", &leaf_out("sp", 0)) && has_edge(&graph, "b1", &leaf_out("sp", 1))
    );
    assert_eq!(node(&graph, &leaf_out("sp", 0)).label, "Out A");
    assert_eq!(node(&graph, &leaf_out("sp", 1)).label, "Out B");
}

#[test]
fn empty_mix_lanes_still_draw_split_and_mixer() {
    let graph = chain_graph(
        &chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]),
        &labels(),
    );
    assert!(has_edge(&graph, FIRST_SPLIT_NODE_ID, FIRST_MIXER_NODE_ID));
    assert!(has_edge(&graph, FIRST_MIXER_NODE_ID, OUTPUT_NODE_ID));
}

#[test]
fn a_switched_off_block_is_drawn_bypassed() {
    let mut off = core("a");
    off.enabled = false;
    let graph = chain_graph(&chain(vec![off]), &labels());
    assert!(node(&graph, "a").bypass);
}

#[test]
fn the_grid_centres_the_shared_lane_between_two_path_lanes() {
    assert_eq!(grid_metrics(1).origin_y, CARD_HALF);
    assert_eq!(grid_metrics(2).origin_y, CARD_HALF + LANE_SPACING / 2.0);
}

/// The card face, the "+" of an empty segment and the "only blocks move" rule
/// of `resolve_drop_anchor` all read Part 5's `NodeKind`.
#[test]
fn every_node_carries_its_kind() {
    let y = chain_graph(&y_chain(), &labels());
    assert_eq!(node(&y, INPUT_NODE_ID).kind, NodeKind::IoInput);
    assert_eq!(node(&y, &leaf_out("sp", 0)).kind, NodeKind::IoOutput);
    assert_eq!(node(&y, &leaf_out("sp", 1)).kind, NodeKind::IoOutput);
    assert_eq!(node(&y, FIRST_SPLIT_NODE_ID).kind, NodeKind::Split);
    assert_eq!(node(&y, "a1").kind, NodeKind::Block);
    let mix = chain_graph(&mix_chain(), &labels());
    assert_eq!(node(&mix, FIRST_MIXER_NODE_ID).kind, NodeKind::Mixer);
    assert_eq!(node(&mix, OUTPUT_NODE_ID).kind, NodeKind::IoOutput);
}

/// Pins the stage numbering Task 4's `insert_target` counts on: every "+"
/// Part 5 places on a laid-out chain names a place in that chain.
#[test]
fn every_plus_of_a_laid_out_chain_names_a_place_in_it() {
    let chains = [
        chain(vec![core("a"), core("b")]),
        mix_chain(),
        y_chain(),
        chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]),
    ];
    for c in chains {
        let graph = chain_graph(&c, &labels());
        assert!(!graph.anchors.is_empty(), "no + on {:?}", c.blocks);
        for anchor in &graph.anchors {
            let slot =
                parse_anchor(&anchor.id).unwrap_or_else(|| panic!("unparsed anchor {}", anchor.id));
            assert!(
                insert_target(&c, &slot).is_some(),
                "anchor {} names no place",
                anchor.id
            );
        }
    }
}

/// The DIGITAL chain on the rig: `amp → split Y (A: cab, filter, dyn | B: —)`.
/// The shared blocks sit on the input's lane, one column each, before the
/// split — the amp was drawn under the split, off the lane.
#[test]
fn shared_blocks_before_a_y_split_sit_on_the_input_lane() {
    // Block ids as the rig writes them: `rig:<input>:block:<uuid>`.
    let amp_id = "rig:input-4:block:990da0ea";
    let cab_id = "rig:input-4:block:62894916";
    let split_id = "rig:input-4:block:c723fb6e";
    let graph = chain_graph(
        &chain(vec![
            core(amp_id),
            split(
                split_id,
                SplitEnd::Y,
                vec![
                    core(cab_id),
                    core("rig:input-4:block:64a54f6d"),
                    core("rig:input-4:block:f5928419"),
                ],
                vec![],
            ),
        ]),
        &labels(),
    );
    let input = node(&graph, INPUT_NODE_ID);
    let amp = node(&graph, amp_id);
    let sp = node(&graph, &split_node_id(split_id));
    assert_eq!(
        (amp.y, sp.y),
        (input.y, input.y),
        "input, amp and split share one lane"
    );
    assert!(
        input.x < amp.x && amp.x < sp.x && sp.x < node(&graph, cab_id).x,
        "left to right: input {} → amp {} → split {} → cab {}",
        input.x,
        amp.x,
        sp.x,
        node(&graph, cab_id).x
    );
    let a_out = node(&graph, &leaf_out(split_id, 0));
    let b_out = node(&graph, &leaf_out(split_id, 1));
    assert!(
        a_out.y < sp.y && sp.y < b_out.y,
        "lane A above, lane B below"
    );
}
