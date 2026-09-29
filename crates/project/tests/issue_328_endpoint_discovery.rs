//! #328 — `resolve_chain_ports` applies the graph's checklists (spec §1.3). An
//! endpoint unchecked on its node opens no port, so it opens no stream, builds
//! no segment and claims no capture tap. A Y chain has no chain output node:
//! a tail output lives while either path's output node keeps it. Mid ports
//! (#85) are not on the checklist.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::{AudioBlock, AudioBlockKind, OutputBlock, SplitBlock, SplitEnd};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointNode, EndpointRef};

fn ep(name: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![ep("in 1", 0), ep("in 2", 1)],
        outputs: vec![ep("out L", 0), ep("out R", 1)],
    }]
}

fn chain(blocks: Vec<AudioBlock>, unchecked: &[(EndpointNode, &str)]) -> Chain {
    let mut disabled_endpoints = EndpointDisables::default();
    for (node, endpoint) in unchecked {
        disabled_endpoints.set_enabled(
            *node,
            EndpointRef {
                io: "io".into(),
                endpoint: (*endpoint).into(),
            },
            false,
        );
    }
    Chain {
        id: ChainId("rig:g".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn split(end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id: BlockId("split".into()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::new(end)),
    }
}

fn names(chain: &Chain, direction: PortDirection) -> Vec<String> {
    resolve_chain_ports(chain, &registry())
        .into_iter()
        .filter(|p| p.direction == direction)
        .map(|p| p.endpoint.name)
        .collect()
}

#[test]
fn an_input_unchecked_on_the_input_node_opens_no_port() {
    let c = chain(vec![], &[(EndpointNode::Input, "in 2")]);
    assert_eq!(names(&c, PortDirection::Input), vec!["in 1"]);
    assert_eq!(
        names(&c, PortDirection::Output),
        vec!["out L", "out R"],
        "the output node is untouched"
    );
}

#[test]
fn an_output_unchecked_on_the_output_node_opens_no_port() {
    let c = chain(vec![], &[(EndpointNode::Output, "out R")]);
    assert_eq!(names(&c, PortDirection::Output), vec!["out L"]);
}

#[test]
fn a_mix_chain_ends_at_the_chain_output_node() {
    let c = chain(
        vec![split(SplitEnd::Mix)],
        &[
            (EndpointNode::PathAOutput, "out L"),
            (EndpointNode::Output, "out R"),
        ],
    );
    assert_eq!(
        names(&c, PortDirection::Output),
        vec!["out L"],
        "a Mix chain reads the chain output node, not the path nodes"
    );
}

#[test]
fn a_y_output_lives_while_either_path_keeps_it() {
    let c = chain(
        vec![split(SplitEnd::Y)],
        &[
            (EndpointNode::PathAOutput, "out L"),
            (EndpointNode::PathBOutput, "out L"),
            (EndpointNode::PathBOutput, "out R"),
            (EndpointNode::Output, "out R"),
        ],
    );
    assert_eq!(
        names(&c, PortDirection::Output),
        vec!["out R"],
        "out L is fed by no path; out R is still fed by path A (the chain output node does not exist on a Y chain)"
    );
}

#[test]
fn unchecking_every_input_leaves_the_chain_without_inputs() {
    let c = chain(
        vec![],
        &[(EndpointNode::Input, "in 1"), (EndpointNode::Input, "in 2")],
    );
    assert!(
        names(&c, PortDirection::Input).is_empty(),
        "no input port — never a fallback to another endpoint"
    );
}

#[test]
fn a_mid_port_is_not_on_the_checklist() {
    let aux = AudioBlock {
        id: BlockId("aux".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "io".into(),
            endpoint: "out R".into(),
        }),
    };
    let c = chain(vec![aux], &[(EndpointNode::Output, "out R")]);
    let outputs: Vec<(String, bool)> = resolve_chain_ports(&c, &registry())
        .into_iter()
        .filter(|p| p.direction == PortDirection::Output)
        .map(|p| (p.endpoint.name, p.from_block))
        .collect();
    assert_eq!(
        outputs,
        vec![("out L".to_string(), false), ("out R".to_string(), true)],
        "the tail out R is unchecked; the mid port on out R stays"
    );
}
