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
