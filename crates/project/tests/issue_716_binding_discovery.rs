//! #716 Stage 2 — discovery: a chain that SELECTS I/O bindings (by id) has its
//! input/output endpoints resolved FROM those bindings, instead of carrying its
//! own per-block I/O. The engine is unchanged: discovery synthesises the bound
//! Input/Output blocks (head/tail, `io`=binding id, `endpoint`=endpoint name)
//! that `resolve_chain_streams` already consumes.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::binding_discovery::resolve_bound_io_blocks;
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

#[test]
fn selected_binding_expands_into_head_inputs_and_tail_outputs() {
    let chain = chain_with(vec!["xyz".into()], vec![effect("A")]);
    let resolved = resolve_bound_io_blocks(&chain, &registry());

    // Inputs (head): one bound Input block per binding input endpoint.
    let inputs = resolved.input_blocks();
    assert_eq!(inputs.len(), 2, "two input endpoints → two input blocks");
    assert_eq!(inputs[0].1.io, "xyz");
    assert_eq!(inputs[0].1.endpoint, "ch1");
    assert_eq!(inputs[1].1.endpoint, "ch2");

    // Outputs (tail): one bound Output block per binding output endpoint.
    let outputs = resolved.output_blocks();
    assert_eq!(outputs.len(), 1, "one output endpoint → one output block");
    assert_eq!(outputs[0].1.io, "xyz");
    assert_eq!(outputs[0].1.endpoint, "ch3");

    // The effect block survives, between inputs and outputs.
    let order: Vec<&str> = resolved
        .blocks
        .iter()
        .map(|b| match &b.kind {
            AudioBlockKind::Input(i) => i.endpoint.as_str(),
            AudioBlockKind::Output(o) => o.endpoint.as_str(),
            AudioBlockKind::Core(_) => "A",
            _ => "?",
        })
        .collect();
    assert_eq!(order, vec!["ch1", "ch2", "A", "ch3"]);
}

#[test]
fn empty_io_binding_ids_leaves_chain_unchanged() {
    // Legacy chain: keeps whatever blocks it already had (no discovery).
    let chain = chain_with(vec![], vec![effect("A")]);
    let resolved = resolve_bound_io_blocks(&chain, &registry());
    assert_eq!(resolved.blocks.len(), 1);
    assert!(matches!(resolved.blocks[0].kind, AudioBlockKind::Core(_)));
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
    let resolved = resolve_bound_io_blocks(&chain, &reg);

    let inputs = resolved.input_blocks();
    assert_eq!(inputs.len(), 3, "xyz(ch1,ch2)+abc(mic)");
    assert_eq!(inputs[2].1.endpoint, "mic");
    let outputs = resolved.output_blocks();
    assert_eq!(outputs.len(), 2, "xyz(ch3)+abc(amp)");
    assert_eq!(outputs[1].1.endpoint, "amp");
}

#[test]
fn unknown_binding_id_is_skipped() {
    let chain = chain_with(vec!["ghost".into()], vec![effect("A")]);
    let resolved = resolve_bound_io_blocks(&chain, &registry());
    assert!(resolved.input_blocks().is_empty(), "unknown binding adds nothing");
    assert!(resolved.output_blocks().is_empty());
}
