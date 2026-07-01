//! #716 discovery: a chain that SELECTS I/O bindings (by id) has its
//! input/output PORTS resolved FROM those bindings, instead of carrying its own
//! per-block device I/O. `resolve_chain_ports` materializes the head inputs and
//! tail outputs from the selected bindings (plus any mid Input/Output blocks
//! that reference a binding endpoint), reading the device endpoint from the
//! per-machine registry.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

fn ep(name: &str, ch: usize) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    }
}

fn effect(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn chain_with(io_binding_ids: Vec<String>, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids,
        blocks,
        di_output: None,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "xyz".into(),
        name: "XYZ".into(),
        inputs: vec![ep("ch1", 0), ep("ch2", 1)],
        outputs: vec![ep("ch3", 2)],
    }]
}

fn endpoint_names(ports: &[project::binding_discovery::ChainPort], dir: PortDirection) -> Vec<String> {
    ports
        .iter()
        .filter(|p| p.direction == dir)
        .map(|p| p.endpoint.name.clone())
        .collect()
}

#[test]
fn selected_binding_expands_into_head_inputs_and_tail_outputs() {
    let chain = chain_with(vec!["xyz".into()], vec![effect("A")]);
    let ports = resolve_chain_ports(&chain, &registry());

    // Inputs (head): one port per binding input endpoint, at offset 0.
    let inputs: Vec<_> = ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input)
        .collect();
    assert_eq!(inputs.len(), 2, "two input endpoints → two input ports");
    assert!(inputs.iter().all(|p| p.offset == 0 && p.binding_id == "xyz"));
    assert_eq!(
        endpoint_names(&ports, PortDirection::Input),
        vec!["ch1".to_string(), "ch2".to_string()]
    );

    // Outputs (tail): one port per binding output endpoint, at the tail offset.
    let outputs: Vec<_> = ports
        .iter()
        .filter(|p| p.direction == PortDirection::Output)
        .collect();
    assert_eq!(outputs.len(), 1, "one output endpoint → one output port");
    assert_eq!(outputs[0].offset, chain.blocks.len(), "output sits at the tail");
    assert_eq!(outputs[0].binding_id, "xyz");
    assert_eq!(outputs[0].endpoint.name, "ch3");
}

#[test]
fn empty_io_binding_ids_resolves_no_ports() {
    // Legacy chain with no binding selection and no bound mid blocks: nothing
    // to discover.
    let chain = chain_with(vec![], vec![effect("A")]);
    let ports = resolve_chain_ports(&chain, &registry());
    assert!(ports.is_empty());
}

#[test]
fn multiple_selected_bindings_concatenate_in_order() {
    let mut reg = registry();
    reg.push(IoBinding {
        id: "abc".into(),
        name: "ABC".into(),
        inputs: vec![ep("mic", 0)],
        outputs: vec![ep("amp", 1)],
    });
    let chain = chain_with(vec!["xyz".into(), "abc".into()], vec![effect("A")]);
    let ports = resolve_chain_ports(&chain, &reg);

    assert_eq!(
        endpoint_names(&ports, PortDirection::Input),
        vec!["ch1".to_string(), "ch2".to_string(), "mic".to_string()],
        "xyz(ch1,ch2) then abc(mic)"
    );
    assert_eq!(
        endpoint_names(&ports, PortDirection::Output),
        vec!["ch3".to_string(), "amp".to_string()],
        "xyz(ch3) then abc(amp)"
    );
}

#[test]
fn unknown_binding_id_is_skipped() {
    let chain = chain_with(vec!["ghost".into()], vec![effect("A")]);
    let ports = resolve_chain_ports(&chain, &registry());
    assert!(ports.is_empty(), "unknown binding resolves no ports");
}

#[test]
fn mid_input_block_resolves_its_binding_endpoint() {
    // A manually inserted Input block references a binding endpoint by
    // io/endpoint; discovery resolves it at the block's own offset.
    let mid = AudioBlock {
        id: BlockId("mid:in".into()),
        enabled: true,
        kind: AudioBlockKind::Input(project::block::InputBlock {
            model: "standard".into(),
            io: "xyz".into(),
            endpoint: "ch2".into(),
        }),
    };
    let chain = chain_with(vec![], vec![effect("A"), mid, effect("B")]);
    let ports = resolve_chain_ports(&chain, &registry());

    assert_eq!(ports.len(), 1, "only the bound mid block resolves a port");
    assert_eq!(ports[0].direction, PortDirection::Input);
    assert_eq!(ports[0].offset, 1, "port sits at the mid block's index");
    assert_eq!(ports[0].endpoint.name, "ch2");
}
