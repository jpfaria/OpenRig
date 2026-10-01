//! #328 (spec §5.2) — the graph a row draws: one Slint node per laid-out node,
//! the node's kind, its block tile data, wires that join node centres.

use super::*;
use crate::chain_graph_adapter::chain_graph;
use crate::chain_graph_adapter::LANE_SPACING;
use crate::chain_graph_fixtures_tests::{
    chain, core, mix_chain, registry, rows, split_paths, FIRST_MIXER_NODE_ID, FIRST_SPLIT_NODE_ID,
};
use crate::chain_graph_ids::{INPUT_NODE_ID, OUTPUT_NODE_ID};
use crate::endpoint_checklist_items::IoLabels;
use crate::project_view::replace_project_chains;
use project::block::SplitEnd;
use project::project::Project;
use slint::Model;

fn labels() -> IoLabels {
    IoLabels {
        input: "In".into(),
        output: "Out".into(),
        leaves: Vec::new(),
    }
}

fn models_of(c: &project::chain::Chain) -> RowGraphModels {
    // Building a tile reads the asset paths (thumbnails), which panic until
    // startup set them (same guard as `block_delete_tests.rs:70-75`).
    infra_filesystem::init_asset_paths(infra_filesystem::AssetPaths::default());
    row_graph_models(c, &chain_graph(c, &labels()))
}

#[test]
fn each_node_carries_its_kind_and_its_block_tile() {
    let c = mix_chain();
    let models = models_of(&c);
    let nodes: Vec<GraphNode> = models.nodes.iter().collect();
    let find = |id: &str| nodes.iter().find(|n| n.id.as_str() == id).cloned().unwrap();
    assert_eq!(find(INPUT_NODE_ID).kind.as_str(), "io_input");
    assert_eq!(find(FIRST_SPLIT_NODE_ID).kind.as_str(), "split");
    assert_eq!(find(FIRST_MIXER_NODE_ID).kind.as_str(), "mixer");
    assert_eq!(find(OUTPUT_NODE_ID).kind.as_str(), "io_output");
    let a1 = find("a1");
    assert_eq!(a1.kind.as_str(), "block");
    // The card and the canvas tooltip read the strip's own tile (Part 5).
    let tile = crate::chain_block_item::chain_block_item_from_block(&core("a1"));
    assert_eq!(
        (
            a1.block.icon_kind.as_str(),
            a1.block.type_label.as_str(),
            a1.block.display_name.as_str()
        ),
        (
            tile.icon_kind.as_str(),
            tile.type_label.as_str(),
            tile.display_name.as_str()
        ),
    );
    assert_eq!(
        find(FIRST_SPLIT_NODE_ID).block.display_name.as_str(),
        "",
        "no hover tooltip on routing nodes"
    );
}

#[test]
fn every_plus_is_published_where_part_5_placed_it() {
    let c = mix_chain();
    let graph = chain_graph(&c, &labels());
    let models = models_of(&c);
    let anchors: Vec<crate::GraphAnchor> = models.anchors.iter().collect();
    assert_eq!(anchors.len(), graph.anchors.len());
    for (published, placed) in anchors.iter().zip(&graph.anchors) {
        assert_eq!(published.id.as_str(), placed.id);
        assert_eq!(
            (published.layout_x, published.layout_y),
            (placed.x, placed.y)
        );
        assert_eq!(published.always_visible, placed.always_visible);
    }
}

#[test]
fn every_wire_joins_its_two_node_centres() {
    let c = mix_chain();
    let models = models_of(&c);
    let nodes: Vec<GraphNode> = models.nodes.iter().collect();
    let centre = |id: &str| {
        let n = nodes.iter().find(|n| n.id.as_str() == id).unwrap();
        (n.layout_x, n.layout_y)
    };
    assert_eq!(models.edges.row_count(), 9);
    for edge in models.edges.iter() {
        assert_eq!((edge.from_x, edge.from_y), centre(edge.from_id.as_str()));
        assert_eq!((edge.to_x, edge.to_y), centre(edge.to_id.as_str()));
    }
}

/// An empty path draws its own lane: its "+" sits on its own row, below the
/// lanes before it, and its wire bends through that "+" instead of running
/// straight over another lane.
#[test]
fn an_empty_path_draws_its_own_lane() {
    let c = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![vec![core("a1")], vec![], vec![]],
    )]);
    let models = models_of(&c);
    let nodes: Vec<GraphNode> = models.nodes.iter().collect();
    let a1_y = nodes
        .iter()
        .find(|n| n.id.as_str() == "a1")
        .unwrap()
        .layout_y;
    let anchors: Vec<crate::GraphAnchor> = models.anchors.iter().collect();
    let plus = |lane: usize| {
        let id = format!("path:sp:{lane}:0");
        anchors
            .iter()
            .find(|a| a.id.as_str() == id)
            .unwrap_or_else(|| panic!("no + {id}"))
            .clone()
    };
    let (b, c_lane) = (plus(1), plus(2));
    assert_eq!(
        (b.layout_y - a1_y, c_lane.layout_y - b.layout_y),
        (LANE_SPACING, LANE_SPACING),
        "lanes A, B, C one row apart, top to bottom"
    );
    for (lane, p) in [(1, &b), (2, &c_lane)] {
        assert!(
            models
                .edges
                .iter()
                .any(|e| (e.to_x, e.to_y) == (p.layout_x, p.layout_y)),
            "a wire runs through lane {lane}'s +"
        );
    }
}

#[test]
fn a_rebuilt_row_carries_its_chain_graph() {
    let rows = rows();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain(vec![core("a"), core("b")]), mix_chain()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    let linear = rows.row_data(0).unwrap();
    assert_eq!(linear.graph_nodes.row_count(), 4);
    assert_eq!((linear.graph_lanes, linear.graph_columns), (1, 4));
    let split = rows.row_data(1).unwrap();
    assert_eq!(split.graph_lanes, 2);
    // mix_chain: 9 wires, one "+" each.
    assert_eq!(split.graph_anchors.row_count(), 9);
}
