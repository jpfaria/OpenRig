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
