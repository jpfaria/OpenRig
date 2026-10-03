//! #328 (spec §5.3) — the checklist lists every input (or output) of the
//! chain's E/S, checked unless that node disabled it; a mid-chain port is not
//! part of the input/output node.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, port_in, registry, y_chain};
use domain::ids::BlockId;
use project::block::PathRef;
use project::endpoint_disables::{EndpointNode, EndpointRef};

fn r(io: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: io.into(),
        endpoint: endpoint.into(),
    }
}

fn leaf(path: usize) -> PathRef {
    PathRef {
        split: BlockId("sp".into()),
        path,
    }
}

fn labels(rows: &[EndpointRow]) -> Vec<(&str, bool)> {
    rows.iter()
        .map(|row| (row.label.as_str(), row.enabled))
        .collect()
}

#[test]
fn the_input_node_lists_the_chains_inputs_not_its_mid_ports() {
    let c = chain(vec![port_in("port", "main", "In 2"), core("amp")]);
    let rows = endpoint_rows(&c, &registry(), &EndpointNode::Input);
    assert_eq!(labels(&rows), vec![("In 1", true), ("In 2", true)]);
    assert_eq!(
        (rows[1].io.as_str(), rows[1].endpoint.as_str()),
        ("main", "In 2")
    );
}

#[test]
fn outputs_with_the_same_name_carry_their_binding_name() {
    let rows = endpoint_rows(&chain(vec![]), &registry(), &EndpointNode::Output);
    assert_eq!(
        labels(&rows),
        vec![("Scarlett · Out L/R", true), ("AUX · Out L/R", true)]
    );
}

#[test]
fn an_unchecked_endpoint_stays_listed_disabled() {
    let mut c = chain(vec![]);
    c.disabled_endpoints
        .set_enabled(&EndpointNode::Input, r("main", "In 2"), false);
    let rows = endpoint_rows(&c, &registry(), &EndpointNode::Input);
    assert_eq!(labels(&rows), vec![("In 1", true), ("In 2", false)]);
}

#[test]
fn a_path_output_node_reads_only_its_own_disables() {
    let mut c = y_chain();
    c.disabled_endpoints.set_enabled(
        &EndpointNode::PathOutput(leaf(0)),
        r("aux", "Out L/R"),
        false,
    );
    let a = endpoint_rows(&c, &registry(), &EndpointNode::PathOutput(leaf(0)));
    let b = endpoint_rows(&c, &registry(), &EndpointNode::PathOutput(leaf(1)));
    let main = endpoint_rows(&c, &registry(), &EndpointNode::Output);
    assert_eq!(
        labels(&a),
        vec![("Scarlett · Out L/R", true), ("AUX · Out L/R", false)]
    );
    assert!(
        main.iter().chain(&b).all(|row| row.enabled),
        "the chain output node and the other leaf are untouched"
    );
}

#[test]
fn a_node_label_names_its_checked_endpoints() {
    let rows = vec![
        EndpointRow {
            io: "main".into(),
            endpoint: "In 1".into(),
            label: "In 1".into(),
            enabled: true,
            aliases: vec![],
        },
        EndpointRow {
            io: "main".into(),
            endpoint: "In 2".into(),
            label: "In 2".into(),
            enabled: false,
            aliases: vec![],
        },
    ];
    assert_eq!(node_label(&rows, "None"), "In 1");
    let off: Vec<EndpointRow> = rows
        .into_iter()
        .map(|row| EndpointRow {
            enabled: false,
            ..row
        })
        .collect();
    assert_eq!(node_label(&off, "None"), "None");
}

#[test]
fn without_a_registry_the_nodes_fall_back_to_the_row_labels() {
    let io = io_labels(&y_chain(), &[], "In", "Out");
    assert_eq!((io.input.as_str(), io.output.as_str()), ("In", "Out"));
    assert_eq!((io.leaf(&leaf(0)), io.leaf(&leaf(1))), ("Out", "Out"));
}

#[test]
fn with_a_registry_each_node_names_its_endpoints() {
    let io = io_labels(&chain(vec![]), &registry(), "In", "Out");
    assert_eq!(io.input, "In 1, In 2");
    assert_eq!(io.output, "Scarlett · Out L/R, AUX · Out L/R");
}

/// `main` plus `dup`, a second binding that carries main's Out L/R again
/// (same device + channels) and an input of its own.
fn overlapping_registry() -> Vec<infra_filesystem::IoBinding> {
    let mut registry = registry();
    registry.push(infra_filesystem::IoBinding {
        id: "dup".into(),
        name: "DUP".into(),
        inputs: vec![crate::chain_graph_fixtures_tests::endpoint_at(
            "In 1",
            vec![0, 1],
        )],
        outputs: vec![crate::chain_graph_fixtures_tests::endpoint_at(
            "Out L/R",
            vec![0, 1],
        )],
    });
    registry
}

#[test]
fn an_endpoint_shared_by_two_bindings_is_one_row() {
    let mut c = chain(vec![]);
    c.io_binding_ids = vec!["main".into(), "dup".into()];
    let inputs = endpoint_rows(&c, &overlapping_registry(), &EndpointNode::Input);
    let outputs = endpoint_rows(&c, &overlapping_registry(), &EndpointNode::Output);
    assert_eq!(labels(&inputs), vec![("In 1", true), ("In 2", true)]);
    assert_eq!(labels(&outputs), vec![("Out L/R", true)]);
    assert_eq!(
        outputs[0].aliases,
        vec![r("main", "Out L/R"), r("dup", "Out L/R")]
    );
}

#[test]
fn a_shared_row_is_checked_while_any_copy_still_plays() {
    let mut c = chain(vec![]);
    c.io_binding_ids = vec!["main".into(), "dup".into()];
    c.disabled_endpoints
        .set_enabled(&EndpointNode::Output, r("dup", "Out L/R"), false);
    let outputs = endpoint_rows(&c, &overlapping_registry(), &EndpointNode::Output);
    assert_eq!(labels(&outputs), vec![("Out L/R", true)]);
}
