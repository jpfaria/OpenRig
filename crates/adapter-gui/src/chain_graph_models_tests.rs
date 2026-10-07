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
        ports: Vec::new(),
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

/// #398: every wire names the lane it runs in, so the canvas colours lane A
/// and lane B apart; the trunk before and after the split is -1.
#[test]
fn every_wire_names_its_lane() {
    let c = mix_chain();
    let models = models_of(&c);
    let lane = |from: &str, to: &str| {
        models
            .edges
            .iter()
            .find(|e| e.from_id.as_str() == from && e.to_id.as_str() == to)
            .unwrap_or_else(|| panic!("no wire {from} → {to}"))
            .path
    };
    assert_eq!(lane(INPUT_NODE_ID, "pre"), -1);
    assert_eq!(lane("pre", FIRST_SPLIT_NODE_ID), -1);
    assert_eq!(lane(FIRST_SPLIT_NODE_ID, "a1"), 0);
    assert_eq!(lane("a1", "a2"), 0);
    assert_eq!(lane("a2", FIRST_MIXER_NODE_ID), 0);
    assert_eq!(lane(FIRST_SPLIT_NODE_ID, "b1"), 1);
    assert_eq!(lane("b1", FIRST_MIXER_NODE_ID), 1);
    assert_eq!(lane(FIRST_MIXER_NODE_ID, "post"), -1);

    let empty = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![vec![core("a1")], vec![]],
    )]);
    let models = models_of(&empty);
    assert!(
        models
            .edges
            .iter()
            .filter(|e| e.from_id.as_str() == "" || e.to_id.as_str() == "")
            .all(|e| e.path == 1),
        "the empty lane's bent wire is lane 1"
    );
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

fn wire_between(models: &RowGraphModels, from: &str, to: &str) -> GraphEdgeGeometry {
    models
        .edges
        .iter()
        .find(|e| e.from_id.as_str() == from && e.to_id.as_str() == to)
        .unwrap_or_else(|| panic!("no wire {from} → {to}"))
}

fn node_of(models: &RowGraphModels, id: &str) -> GraphNode {
    models
        .nodes
        .iter()
        .find(|n| n.id.as_str() == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn lanes_of(models: &RowGraphModels, id: &str) -> Vec<String> {
    node_of(models, id)
        .lanes
        .iter()
        .map(|lane| lane.to_string())
        .collect()
}

/// #398: a wire into or out of a split or a mixer plugs into that hub's
/// port for its path; every other end is the node's own centre line (-1).
#[test]
fn every_wire_plugs_into_its_hubs_port() {
    let c = mix_chain();
    let models = models_of(&c);
    let ports = |from: &str, to: &str| {
        let e = wire_between(&models, from, to);
        (e.from_port, e.to_port)
    };
    assert_eq!(ports(INPUT_NODE_ID, "pre"), (-1, -1));
    assert_eq!(ports("pre", FIRST_SPLIT_NODE_ID), (-1, -1));
    assert_eq!(ports(FIRST_SPLIT_NODE_ID, "a1"), (0, -1));
    assert_eq!(ports(FIRST_SPLIT_NODE_ID, "b1"), (1, -1));
    assert_eq!(ports("a1", "a2"), (-1, -1));
    assert_eq!(ports("a2", FIRST_MIXER_NODE_ID), (-1, 0));
    assert_eq!(ports("b1", FIRST_MIXER_NODE_ID), (-1, 1));
    assert_eq!(ports(FIRST_MIXER_NODE_ID, "post"), (-1, -1));
}

/// #398: the wire bent through an empty path's "+" leaves the split and
/// enters the mixer at that path's port.
#[test]
fn a_wire_through_an_empty_path_plugs_into_that_paths_ports() {
    let c = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![vec![core("a1")], vec![]],
    )]);
    let models = models_of(&c);
    let out_of_split = models
        .edges
        .iter()
        .find(|e| e.to_id.as_str() == "")
        .expect("a wire into the empty path's bend");
    let into_mixer = models
        .edges
        .iter()
        .find(|e| e.from_id.as_str() == "")
        .expect("a wire out of the empty path's bend");
    assert_eq!(
        (out_of_split.from_id.as_str(), out_of_split.from_port),
        (FIRST_SPLIT_NODE_ID, 1)
    );
    assert_eq!(
        (into_mixer.to_id.as_str(), into_mixer.to_port),
        (FIRST_MIXER_NODE_ID, 1)
    );
}

/// #398: a split nested in a path plugs into its parent's port for that
/// path, on both ends.
#[test]
fn a_nested_split_plugs_into_its_parents_path_port() {
    let inner = split_paths("in", SplitEnd::Mix, vec![vec![core("x")], vec![core("y")]]);
    let c = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![vec![core("b0")], vec![inner]],
    )]);
    let models = models_of(&c);
    let ports = |from: &str, to: &str| {
        let e = wire_between(&models, from, to);
        (e.from_port, e.to_port)
    };
    assert_eq!(ports(FIRST_SPLIT_NODE_ID, "__split_in"), (1, -1));
    assert_eq!(ports("__split_in", "y"), (1, -1));
    assert_eq!(ports("__merge_in", FIRST_MIXER_NODE_ID), (-1, 1));
}

/// #398: a wire names the nodes it joins by their index in the node list, so
/// the canvas can end it on that node's edge; a bend point is -1.
#[test]
fn every_wire_names_its_end_nodes_by_index() {
    let c = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![vec![core("a1")], vec![]],
    )]);
    let models = models_of(&c);
    let nodes: Vec<GraphNode> = models.nodes.iter().collect();
    for edge in models.edges.iter() {
        for (id, index) in [
            (edge.from_id.clone(), edge.from_index),
            (edge.to_id.clone(), edge.to_index),
        ] {
            if id.is_empty() {
                assert_eq!(index, -1, "a bend point is no node");
            } else {
                assert_eq!(nodes[index as usize].id, id);
            }
        }
    }
}

/// #398: a split or mixer hub carries one letter per path, however many
/// paths it has, and its wires plug into one port each.
#[test]
fn a_hub_carries_one_letter_per_path() {
    let c = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![
            vec![core("a")],
            vec![core("b")],
            vec![core("c")],
            vec![core("d")],
        ],
    )]);
    let models = models_of(&c);
    assert_eq!(lanes_of(&models, FIRST_SPLIT_NODE_ID), ["A", "B", "C", "D"]);
    assert_eq!(lanes_of(&models, FIRST_MIXER_NODE_ID), ["A", "B", "C", "D"]);
    assert!(lanes_of(&models, "a").is_empty());
    for (port, id) in ["a", "b", "c", "d"].into_iter().enumerate() {
        assert_eq!(
            wire_between(&models, FIRST_SPLIT_NODE_ID, id).from_port,
            port as i32
        );
        assert_eq!(
            wire_between(&models, id, FIRST_MIXER_NODE_ID).to_port,
            port as i32
        );
    }
}

/// #398: the first node of each path carries that path's tag (PATH A over
/// lane A, PATH B under lane B); every other node carries none.
#[test]
fn the_first_node_of_each_path_carries_its_path_tag() {
    let c = mix_chain();
    let models = models_of(&c);
    let tag = |id: &str| {
        let n = node_of(&models, id);
        (n.path_tag.to_string(), n.path_index)
    };
    assert_eq!(tag("a1"), ("A".to_string(), 0));
    assert_eq!(tag("b1"), ("B".to_string(), 1));
    assert_eq!(tag("a2").0, "");
    assert_eq!(tag("pre").0, "");
    assert_eq!(tag(FIRST_SPLIT_NODE_ID).0, "");
}

/// #398: only the outer paths are tagged (PATH A above the top lane, the
/// last path under the bottom lane). A middle lane has a lane on either side
/// and no room for a tag; its hub letter and wire colour name it.
#[test]
fn a_middle_path_goes_untagged() {
    let c = chain(vec![split_paths(
        "sp",
        SplitEnd::Mix,
        vec![vec![core("a")], vec![core("b")], vec![core("c")]],
    )]);
    let models = models_of(&c);
    let tag = |id: &str| node_of(&models, id).path_tag.to_string();
    assert_eq!(tag("a"), "A");
    assert_eq!(tag("b"), "");
    assert_eq!(tag("c"), "C");
}
