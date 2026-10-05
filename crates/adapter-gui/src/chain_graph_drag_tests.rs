//! #328 — the dragged card and the ends of its wires follow the pointer.

use super::*;
use crate::chain_graph_fixtures_tests::{mix_chain, registry, rows};
use crate::project_view::replace_project_chains;
use project::project::Project;

#[test]
fn a_dragged_node_carries_the_ends_of_its_wires() {
    let rows = rows();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![mix_chain()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    move_node(&rows, 0, "a1", 999.0, 777.0);
    let row = rows.row_data(0).unwrap();
    let a1 = row
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "a1")
        .unwrap();
    assert_eq!((a1.layout_x, a1.layout_y), (999.0, 777.0));
    for edge in row.graph_edges.iter() {
        if edge.to_id.as_str() == "a1" {
            assert_eq!((edge.to_x, edge.to_y), (999.0, 777.0));
        }
        if edge.from_id.as_str() == "a1" {
            assert_eq!((edge.from_x, edge.from_y), (999.0, 777.0));
        }
    }
}

#[test]
fn a_released_drag_puts_every_card_back_on_the_layout() {
    let rows = rows();
    let chain = mix_chain();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    let laid_out: Vec<(String, f32, f32)> = rows
        .row_data(0)
        .unwrap()
        .graph_nodes
        .iter()
        .map(|n| (n.id.to_string(), n.layout_x, n.layout_y))
        .collect();

    move_node(&rows, 0, "a1", 999.0, 777.0);
    move_node(&rows, 0, "sp", 3.0, 4.0);
    settle_nodes(&rows, 0, &chain);

    let row = rows.row_data(0).unwrap();
    let settled: Vec<(String, f32, f32)> = row
        .graph_nodes
        .iter()
        .map(|n| (n.id.to_string(), n.layout_x, n.layout_y))
        .collect();
    assert_eq!(
        settled, laid_out,
        "a drag that did not move a block leaves no card off the grid"
    );
    for edge in row.graph_edges.iter() {
        let end = |id: &str| {
            laid_out
                .iter()
                .find(|(n, _, _)| n == id)
                .map(|(_, x, y)| (*x, *y))
        };
        if let Some(to) = end(edge.to_id.as_str()) {
            assert_eq!((edge.to_x, edge.to_y), to, "wire into {}", edge.to_id);
        }
        if let Some(from) = end(edge.from_id.as_str()) {
            assert_eq!(
                (edge.from_x, edge.from_y),
                from,
                "wire out of {}",
                edge.from_id
            );
        }
    }
}

#[test]
fn dragging_a_split_carries_its_whole_group() {
    let rows = rows();
    let chain = mix_chain();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    let at = |rows: &VecModel<crate::ProjectChainItem>| -> Vec<(String, f32, f32)> {
        rows.row_data(0)
            .unwrap()
            .graph_nodes
            .iter()
            .map(|n| (n.id.to_string(), n.layout_x, n.layout_y))
            .collect()
    };
    let before = at(&rows);
    let (_, sx, sy) = before
        .iter()
        .find(|(id, _, _)| id == "__split_sp")
        .cloned()
        .unwrap();

    drag_node(&rows, 0, &chain, "__split_sp", sx + 100.0, sy + 10.0);

    let group = ["__split_sp", "__merge_sp", "a1", "a2", "b1"];
    for ((id, x0, y0), (_, x1, y1)) in before.iter().zip(at(&rows)) {
        let expected = if group.contains(&id.as_str()) {
            (x0 + 100.0, y0 + 10.0)
        } else {
            (*x0, *y0)
        };
        assert_eq!((x1, y1), expected, "node {id}");
    }
    let after = at(&rows);
    let pos = |id: &str| {
        after
            .iter()
            .find(|(n, _, _)| n == id)
            .map(|(_, x, y)| (*x, *y))
    };
    for edge in rows.row_data(0).unwrap().graph_edges.iter() {
        if let Some(p) = pos(edge.from_id.as_str()) {
            assert_eq!(
                (edge.from_x, edge.from_y),
                p,
                "wire out of {}",
                edge.from_id
            );
        }
        if let Some(p) = pos(edge.to_id.as_str()) {
            assert_eq!((edge.to_x, edge.to_y), p, "wire into {}", edge.to_id);
        }
    }
}

#[test]
fn dragging_a_block_moves_only_that_card() {
    let rows = rows();
    let chain = mix_chain();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    drag_node(&rows, 0, &chain, "a1", 999.0, 777.0);
    let row = rows.row_data(0).unwrap();
    let a2 = row
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "a2")
        .unwrap();
    let a1 = row
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "a1")
        .unwrap();
    assert_eq!((a1.layout_x, a1.layout_y), (999.0, 777.0));
    assert_ne!((a2.layout_x, a2.layout_y), (999.0, 777.0));
}

/// A Mix whose path B is empty: its "+" and the bent wire through it sit on
/// that lane, not on any card.
fn empty_lane_chain() -> project::chain::Chain {
    use crate::chain_graph_fixtures_tests::{chain, core, split};
    use project::block::SplitEnd;
    chain(vec![
        core("pre"),
        split("sp", SplitEnd::Mix, vec![core("a1")], vec![]),
        core("post"),
    ])
}

#[test]
fn dragging_a_split_carries_the_plus_and_the_wire_of_its_empty_lane() {
    let rows = rows();
    let chain = empty_lane_chain();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    let row = rows.row_data(0).unwrap();
    let plus = |row: &crate::ProjectChainItem| {
        row.graph_anchors
            .iter()
            .find(|a| a.id.as_str() == "path:sp:1:0")
            .map(|a| (a.layout_x, a.layout_y))
            .expect("the empty lane's +")
    };
    let bends = |row: &crate::ProjectChainItem| -> Vec<(f32, f32)> {
        row.graph_edges
            .iter()
            .flat_map(|e| {
                let mut ends = Vec::new();
                if e.from_id.is_empty() {
                    ends.push((e.from_x, e.from_y));
                }
                if e.to_id.is_empty() {
                    ends.push((e.to_x, e.to_y));
                }
                ends
            })
            .collect()
    };
    let (px, py) = plus(&row);
    let before_bends = bends(&row);
    assert!(!before_bends.is_empty(), "the empty lane's wire bends");
    let split = row
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "__split_sp")
        .unwrap();

    drag_node(
        &rows,
        0,
        &chain,
        "__split_sp",
        split.layout_x + 100.0,
        split.layout_y + 10.0,
    );

    let row = rows.row_data(0).unwrap();
    assert_eq!(
        plus(&row),
        (px + 100.0, py + 10.0),
        "the + follows the split"
    );
    let moved: Vec<(f32, f32)> = before_bends
        .iter()
        .map(|(x, y)| (x + 100.0, y + 10.0))
        .collect();
    assert_eq!(bends(&row), moved, "the bent wire follows the split");
    let pre = row
        .graph_anchors
        .iter()
        .find(|a| a.id.as_str() == "top:0")
        .map(|a| (a.layout_x, a.layout_y));
    let fresh = crate::chain_graph_fixtures_tests::rows();
    replace_project_chains(&fresh, &project, &[], &[], &registry());
    let laid = fresh
        .row_data(0)
        .unwrap()
        .graph_anchors
        .iter()
        .find(|a| a.id.as_str() == "top:0")
        .map(|a| (a.layout_x, a.layout_y));
    assert_eq!(pre, laid, "a + outside the split stays put");
}

#[test]
fn a_released_split_drag_puts_its_empty_lane_back() {
    let rows = rows();
    let chain = empty_lane_chain();
    let project = Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    };
    replace_project_chains(&rows, &project, &[], &[], &registry());
    let marks = |row: &crate::ProjectChainItem| {
        let anchors: Vec<(String, f32, f32)> = row
            .graph_anchors
            .iter()
            .map(|a| (a.id.to_string(), a.layout_x, a.layout_y))
            .collect();
        let edges: Vec<(f32, f32, f32, f32)> = row
            .graph_edges
            .iter()
            .map(|e| (e.from_x, e.from_y, e.to_x, e.to_y))
            .collect();
        (anchors, edges)
    };
    let laid_out = marks(&rows.row_data(0).unwrap());
    let split = rows
        .row_data(0)
        .unwrap()
        .graph_nodes
        .iter()
        .find(|n| n.id.as_str() == "__split_sp")
        .unwrap();

    drag_node(&rows, 0, &chain, "__split_sp", split.layout_x + 50.0, 5.0);
    settle_nodes(&rows, 0, &chain);

    assert_eq!(
        marks(&rows.row_data(0).unwrap()),
        laid_out,
        "every + and wire returns to the layout"
    );
}
