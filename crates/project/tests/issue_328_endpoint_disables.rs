//! #328 — the graph's input/output checklists (spec §1.3). Every endpoint of
//! the chain's own E/S is checked by default; unchecking one leaves it out of
//! THAT node only, and it stays listed.

use domain::ids::{BlockId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::PathRef;
use project::endpoint_candidates::endpoint_candidates;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

fn r(binding: &str, endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: binding.into(),
        endpoint: endpoint.into(),
    }
}

fn leaf(path: usize) -> PathRef {
    PathRef {
        split: BlockId("y".into()),
        path,
    }
}

fn path_out(path: usize) -> EndpointNode {
    EndpointNode::PathOutput(leaf(path))
}

fn ep(name: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

fn binding(id: &str, inputs: &[&str], outputs: &[&str]) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: inputs.iter().map(|n| ep(n)).collect(),
        outputs: outputs.iter().map(|n| ep(n)).collect(),
    }
}

#[test]
fn every_endpoint_is_checked_by_default() {
    let d = EndpointDisables::default();
    assert!(d.is_empty());
    for node in [
        EndpointNode::Input,
        EndpointNode::Output,
        path_out(0),
        path_out(1),
        path_out(7),
    ] {
        assert!(
            d.is_enabled(&node, &r("io", "x")),
            "{node:?} starts checked"
        );
    }
}

#[test]
fn unchecking_leaves_the_endpoint_out_of_that_node_only() {
    let mut d = EndpointDisables::default();
    d.set_enabled(&path_out(0), r("io", "out L"), false);
    assert!(!d.is_enabled(&path_out(0), &r("io", "out L")));
    assert!(
        d.is_enabled(&path_out(1), &r("io", "out L")),
        "path B's node still has it"
    );
    assert!(d.is_enabled(&EndpointNode::Output, &r("io", "out L")));
    assert!(
        d.is_enabled(&path_out(0), &r("other", "out L")),
        "the same name on another E/S is another endpoint"
    );
}

#[test]
fn rechecking_restores_the_default_and_unchecking_twice_records_once() {
    let mut d = EndpointDisables::default();
    d.set_enabled(&EndpointNode::Input, r("io", "in 2"), false);
    d.set_enabled(&EndpointNode::Input, r("io", "in 2"), false);
    assert_eq!(d.inputs, vec![r("io", "in 2")]);
    d.set_enabled(&EndpointNode::Input, r("io", "in 2"), true);
    assert!(d.is_empty());
}

#[test]
fn a_y_tail_output_lives_while_either_path_keeps_it() {
    let mut d = EndpointDisables::default();
    d.set_enabled(&path_out(0), r("io", "out L"), false);
    assert!(
        d.tail_output_enabled(&[leaf(0), leaf(1)], &r("io", "out L")),
        "path B still feeds it"
    );
    d.set_enabled(&path_out(1), r("io", "out L"), false);
    assert!(
        !d.tail_output_enabled(&[leaf(0), leaf(1)], &r("io", "out L")),
        "no path feeds it"
    );
    assert!(
        d.tail_output_enabled(&[], &r("io", "out L")),
        "a linear or Mix chain reads the chain output node"
    );
}

#[test]
fn endpoints_the_bindings_no_longer_offer_are_dropped() {
    let mut d = EndpointDisables::default();
    d.set_enabled(&EndpointNode::Input, r("io", "in 2"), false);
    d.set_enabled(&EndpointNode::Input, r("io", "gone"), false);
    d.set_enabled(&path_out(1), r("io", "gone out"), false);
    d.set_enabled(&EndpointNode::Output, r("io", "out L"), false);
    d.retain_known(&[r("io", "in 1"), r("io", "in 2")], &[r("io", "out L")]);
    assert_eq!(d.inputs, vec![r("io", "in 2")]);
    assert!(
        d.path_outputs.is_empty(),
        "path B's only entry was gone: {:?}",
        d.path_outputs
    );
    assert_eq!(d.outputs, vec![r("io", "out L")]);
}

#[test]
fn the_checklist_writes_only_the_nodes_that_leave_something_out() {
    let mut d = EndpointDisables::default();
    d.set_enabled(&path_out(1), r("io", "out R"), false);
    let yaml = serde_yaml::to_string(&d).expect("serialize");
    assert!(yaml.contains("path_outputs"), "got:\n{yaml}");
    assert!(
        !yaml.contains("inputs:"),
        "empty nodes are not written, got:\n{yaml}"
    );
    assert_eq!(
        serde_yaml::from_str::<EndpointDisables>(&yaml).expect("deserialize"),
        d
    );
    let node = serde_yaml::to_string(&path_out(0)).expect("serialize node");
    assert!(node.contains("path_output"), "got:\n{node}");
    assert_eq!(
        serde_yaml::from_str::<EndpointNode>(&node).expect("deserialize node"),
        path_out(0)
    );
}

#[test]
fn the_candidates_are_every_endpoint_of_the_selected_bindings() {
    let registry = vec![
        binding("io", &["in 1", "in 2"], &["out L", "out R"]),
        binding("fx", &["ret"], &["send"]),
    ];
    let (inputs, outputs) =
        endpoint_candidates(&["io".to_string(), "missing".to_string()], &registry);
    assert_eq!(inputs, vec![r("io", "in 1"), r("io", "in 2")]);
    assert_eq!(outputs, vec![r("io", "out L"), r("io", "out R")]);
}

#[test]
fn removing_a_path_shifts_the_output_nodes_above_it() {
    let mut d = EndpointDisables::default();
    d.set_enabled(&path_out(0), r("io", "a"), false);
    d.set_enabled(&path_out(1), r("io", "b"), false);
    d.set_enabled(&path_out(2), r("io", "c"), false);
    d.shift_after_path_removed(&BlockId("y".into()), 1);
    assert!(!d.is_enabled(&path_out(0), &r("io", "a")));
    assert!(
        !d.is_enabled(&path_out(1), &r("io", "c")),
        "#328 §11: path C became path B and kept its checklist"
    );
    assert!(
        d.is_enabled(&path_out(1), &r("io", "b")),
        "path B's node went with it"
    );
    assert!(d.is_enabled(&path_out(2), &r("io", "c")));
}

#[test]
fn a_file_saved_with_two_named_paths_loads_onto_the_y() {
    use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
    let mut d: EndpointDisables = serde_yaml::from_str(
        "path_a_outputs:\n  - io: io\n    endpoint: out L\npath_b_outputs:\n  - io: io\n    endpoint: out R\n",
    )
    .expect("legacy file loads");
    let y = AudioBlock {
        id: BlockId("y".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::new(SplitEnd::Y)),
    };
    d.adopt_legacy_paths(&[y]);
    assert!(!d.is_enabled(&path_out(0), &r("io", "out L")));
    assert!(!d.is_enabled(&path_out(1), &r("io", "out R")));
    assert!(d.is_enabled(&path_out(0), &r("io", "out R")));
    let yaml = serde_yaml::to_string(&d).expect("serialize");
    assert!(
        !yaml.contains("path_a_outputs"),
        "#328 §11: the legacy names are not written back, got:\n{yaml}"
    );
}
