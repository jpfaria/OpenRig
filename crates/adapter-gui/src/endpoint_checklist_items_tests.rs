//! #328 (spec §5.3) — the checklist lists every input (or output) of the
//! chain's E/S, checked unless that node disabled it; a mid-chain port is not
//! part of the input/output node.

use super::*;
use crate::chain_graph_fixtures_tests::{chain, core, devices, port_in, registry, y_chain};
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

const IN_1: &str = "Quantum HD 8 · In 1/2";
const IN_2: &str = "Quantum HD 8 · In 3/4";
const OUT_12: &str = "Quantum HD 8 · Out 1/2";
const OUT_34: &str = "Quantum HD 8 · Out 3/4";

fn labels(rows: &[EndpointRow]) -> Vec<(&str, bool)> {
    rows.iter()
        .map(|row| (row.label.as_str(), row.enabled))
        .collect()
}

#[test]
fn the_input_node_lists_the_chains_inputs_not_its_mid_ports() {
    let c = chain(vec![port_in("port", "main", "In 2"), core("amp")]);
    let rows = endpoint_rows(&c, &registry(), &devices(), &EndpointNode::Input);
    assert_eq!(labels(&rows), vec![(IN_1, true), (IN_2, true)]);
    assert_eq!(
        (rows[1].io.as_str(), rows[1].endpoint.as_str()),
        ("main", "In 2")
    );
}

#[test]
fn each_output_is_named_by_device_and_channels_like_the_pickers() {
    let rows = endpoint_rows(
        &chain(vec![]),
        &registry(),
        &devices(),
        &EndpointNode::Output,
    );
    assert_eq!(labels(&rows), vec![(OUT_12, true), (OUT_34, true)]);
}

#[test]
fn an_unchecked_endpoint_stays_listed_disabled() {
    let mut c = chain(vec![]);
    c.disabled_endpoints
        .set_enabled(&EndpointNode::Input, r("main", "In 2"), false);
    let rows = endpoint_rows(&c, &registry(), &devices(), &EndpointNode::Input);
    assert_eq!(labels(&rows), vec![(IN_1, true), (IN_2, false)]);
}

#[test]
fn a_path_output_node_reads_only_its_own_disables() {
    let mut c = y_chain();
    c.disabled_endpoints.set_enabled(
        &EndpointNode::PathOutput(leaf(0)),
        r("aux", "Out L/R"),
        false,
    );
    let a = endpoint_rows(
        &c,
        &registry(),
        &devices(),
        &EndpointNode::PathOutput(leaf(0)),
    );
    let b = endpoint_rows(
        &c,
        &registry(),
        &devices(),
        &EndpointNode::PathOutput(leaf(1)),
    );
    let main = endpoint_rows(&c, &registry(), &devices(), &EndpointNode::Output);
    assert_eq!(labels(&a), vec![(OUT_12, true), (OUT_34, false)]);
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
    let io = io_labels(&y_chain(), &[], &devices(), &devices(), "In", "Out");
    assert_eq!((io.input.as_str(), io.output.as_str()), ("In", "Out"));
    assert_eq!((io.leaf(&leaf(0)), io.leaf(&leaf(1))), ("Out", "Out"));
}

#[test]
fn with_a_registry_each_node_names_its_endpoints() {
    let io = io_labels(
        &chain(vec![]),
        &registry(),
        &devices(),
        &devices(),
        "In",
        "Out",
    );
    assert_eq!(io.input, format!("{IN_1}, {IN_2}"));
    assert_eq!(io.output, format!("{OUT_12}, {OUT_34}"));
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
    let inputs = endpoint_rows(
        &c,
        &overlapping_registry(),
        &devices(),
        &EndpointNode::Input,
    );
    let outputs = endpoint_rows(
        &c,
        &overlapping_registry(),
        &devices(),
        &EndpointNode::Output,
    );
    assert_eq!(labels(&inputs), vec![(IN_1, true), (IN_2, true)]);
    assert_eq!(labels(&outputs), vec![(OUT_12, true)]);
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
    let outputs = endpoint_rows(
        &c,
        &overlapping_registry(),
        &devices(),
        &EndpointNode::Output,
    );
    assert_eq!(labels(&outputs), vec![(OUT_12, true)]);
}

/// #398: an insert card is named after its E/S binding, never its kind
/// again ("INSERT" over "INSERT"); #1103: a mid-chain input/output card is
/// named after the device and channels it plays through.
#[test]
fn a_port_block_is_named_after_its_binding() {
    let insert = |id: &str, io: &str| project::block::AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: project::block::AudioBlockKind::Insert(project::block::InsertBlock {
            model: "standard".into(),
            io: io.into(),
        }),
    };
    let c = chain(vec![
        insert("ins", "main"),
        insert("lost", "gone"),
        insert("unset", ""),
        crate::chain_graph_fixtures_tests::port_in("mid", "main", "In 1"),
    ]);
    let io = io_labels(&c, &registry(), &devices(), &devices(), "In", "Out");
    let none = rust_i18n::t!("label-endpoints-none").to_string();
    assert_eq!(io.port("ins"), "Scarlett");
    assert_eq!(
        io.port("lost"),
        "gone",
        "a binding gone from the registry keeps its id"
    );
    assert_eq!(io.port("unset"), none);
    assert_eq!(io.port("mid"), IN_1);

    let graph = crate::chain_graph_adapter::chain_graph(&c, &io);
    let label = |id: &str| {
        graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.label.clone())
            .unwrap_or_else(|| panic!("no node {id}"))
    };
    assert_eq!(label("ins"), "Scarlett");
    assert_eq!(label("mid"), IN_1);
}

/// #1103: a mid-chain input/output card reads the device and channels of its
/// endpoint, like the chain's own input/output rows; an endpoint the binding
/// no longer has falls back to the binding name.
#[test]
fn a_mid_chain_io_block_is_named_after_its_device() {
    let port_out = |id: &str, io: &str, endpoint: &str| project::block::AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: project::block::AudioBlockKind::Output(project::block::OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    };
    let c = chain(vec![
        port_in("in2", "main", "In 2"),
        port_out("out", "aux", "Out L/R"),
        port_in("stale", "main", "In 9"),
    ]);
    let io = io_labels(&c, &registry(), &devices(), &devices(), "In", "Out");
    assert_eq!(io.port("in2"), IN_2);
    assert_eq!(io.port("out"), OUT_34);
    assert_eq!(io.port("stale"), "Scarlett");
}
