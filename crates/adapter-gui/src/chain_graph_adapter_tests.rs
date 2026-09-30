//! #328 (spec §5.1) — left to right: input → shared blocks → split → lanes
//! (A on top) → mixer → shared blocks → output; for Y each lane ends in its
//! own output node.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, mix_chain, split, y_chain};
use crate::chain_graph_ids::{
    INPUT_NODE_ID, MIXER_NODE_ID, OUTPUT_NODE_ID, PATH_A_OUTPUT_NODE_ID, PATH_B_OUTPUT_NODE_ID,
    SPLIT_NODE_ID,
};
use crate::endpoint_checklist_items::IoLabels;
use crate::graph_anchor::{insert_target, parse_anchor};
use crate::graph_view_model::NodeKind;
use project::block::SplitEnd;

fn labels() -> IoLabels {
    IoLabels {
        input: "In 1".into(),
        output: "Out".into(),
        path_a: "Out A".into(),
        path_b: "Out B".into(),
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
    let split_y = node(&graph, SPLIT_NODE_ID).y;
    assert!(
        node(&graph, "a1").y < split_y && split_y < node(&graph, "b1").y,
        "lane A above, lane B below"
    );
    assert_eq!(node(&graph, "pre").y, split_y);
    assert_eq!(node(&graph, "post").y, node(&graph, MIXER_NODE_ID).y);
    for (from, to) in [
        (INPUT_NODE_ID, "pre"),
        ("pre", SPLIT_NODE_ID),
        (SPLIT_NODE_ID, "a1"),
        ("a1", "a2"),
        ("a2", MIXER_NODE_ID),
        (SPLIT_NODE_ID, "b1"),
        ("b1", MIXER_NODE_ID),
        (MIXER_NODE_ID, "post"),
        ("post", OUTPUT_NODE_ID),
    ] {
        assert!(has_edge(&graph, from, to), "missing wire {from} → {to}");
    }
    assert!(graph
        .nodes
        .iter()
        .all(|n| n.id != PATH_A_OUTPUT_NODE_ID && n.id != PATH_B_OUTPUT_NODE_ID));
}

#[test]
fn a_y_chain_ends_each_lane_in_its_own_output_node() {
    let graph = chain_graph(&y_chain(), &labels());
    assert!(graph
        .nodes
        .iter()
        .all(|n| n.id != OUTPUT_NODE_ID && n.id != MIXER_NODE_ID));
    assert_eq!(node(&graph, PATH_A_OUTPUT_NODE_ID).y, node(&graph, "a1").y);
    assert_eq!(node(&graph, PATH_B_OUTPUT_NODE_ID).y, node(&graph, "b1").y);
    assert!(
        has_edge(&graph, "a1", PATH_A_OUTPUT_NODE_ID)
            && has_edge(&graph, "b1", PATH_B_OUTPUT_NODE_ID)
    );
    assert_eq!(node(&graph, PATH_A_OUTPUT_NODE_ID).label, "Out A");
    assert_eq!(node(&graph, PATH_B_OUTPUT_NODE_ID).label, "Out B");
}

#[test]
fn empty_mix_lanes_still_draw_split_and_mixer() {
    let graph = chain_graph(
        &chain(vec![split("sp", SplitEnd::Mix, vec![], vec![])]),
        &labels(),
    );
    assert!(has_edge(&graph, SPLIT_NODE_ID, MIXER_NODE_ID));
    assert!(has_edge(&graph, MIXER_NODE_ID, OUTPUT_NODE_ID));
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
    assert_eq!(node(&y, PATH_A_OUTPUT_NODE_ID).kind, NodeKind::IoOutput);
    assert_eq!(node(&y, PATH_B_OUTPUT_NODE_ID).kind, NodeKind::IoOutput);
    assert_eq!(node(&y, SPLIT_NODE_ID).kind, NodeKind::Split);
    assert_eq!(node(&y, "a1").kind, NodeKind::Block);
    let mix = chain_graph(&mix_chain(), &labels());
    assert_eq!(node(&mix, MIXER_NODE_ID).kind, NodeKind::Mixer);
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
