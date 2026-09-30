//! #328 — unchecking every input (or every output) of a chain is allowed and
//! means "nothing to play" (spec §5.3). The engine used to fill an empty node
//! with its legacy fallback endpoint — mono channel 0 of device "" — and the
//! insert walker let a loop's return stand in for a missing input, feeding the
//! pedal into itself.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;
use project::endpoint_disables::{EndpointDisables, EndpointRef};

use crate::runtime_endpoints::{effective_inputs, effective_outputs, resolve_chain_io};
use crate::runtime_graph::chain_stream_count;
use crate::runtime_segments::split_chain_into_segments;

fn endpoint(name: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![endpoint("in", ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint("out", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![endpoint("ret", ChannelMode::Mono, vec![3])],
            outputs: vec![endpoint("snd", ChannelMode::Mono, vec![3])],
        },
    ]
}

fn off(endpoint: &str) -> EndpointRef {
    EndpointRef {
        io: "main".into(),
        endpoint: endpoint.into(),
    }
}

fn chain(blocks: Vec<AudioBlock>, disabled_endpoints: EndpointDisables) -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks,
        di_output: None,
        loopers: vec![],
        disabled_endpoints,
    }
}

fn insert() -> AudioBlock {
    AudioBlock {
        id: BlockId("loop".into()),
        enabled: true,
        kind: AudioBlockKind::Insert(InsertBlock {
            model: "standard".into(),
            io: "fx".into(),
        }),
    }
}

/// `(effective inputs, effective outputs)` the runtime would open.
fn endpoints(chain: &Chain) -> (usize, usize) {
    let reg = registry();
    let (ri, ro) = resolve_chain_io(chain, &reg);
    (
        effective_inputs(chain, &ri, &reg).0.len(),
        effective_outputs(chain, &ro, &reg).len(),
    )
}

fn segment_count(chain: &Chain) -> usize {
    let reg = registry();
    let (ri, ro) = resolve_chain_io(chain, &reg);
    let (ei, ci, sp, eg) = effective_inputs(chain, &ri, &reg);
    let eo = effective_outputs(chain, &ro, &reg);
    split_chain_into_segments(chain, &ei, &ci, &sp, &eg, &eo, &reg).len()
}

#[test]
fn every_output_unchecked_invents_no_fallback_output() {
    let chain = chain(
        vec![],
        EndpointDisables {
            outputs: vec![off("out")],
            ..EndpointDisables::default()
        },
    );
    assert_eq!(
        endpoints(&chain).1,
        0,
        "#328: the checklist emptied the output node — the legacy fallback output (device \"\") must not appear"
    );
    assert_eq!(
        chain_stream_count(&chain, &registry()),
        0,
        "#328: no output, no stream"
    );
}

#[test]
fn every_input_unchecked_invents_no_fallback_input() {
    let chain = chain(
        vec![],
        EndpointDisables {
            inputs: vec![off("in")],
            ..EndpointDisables::default()
        },
    );
    assert_eq!(
        endpoints(&chain).0,
        0,
        "#328: the checklist emptied the input node — the legacy fallback input must not appear"
    );
    assert_eq!(segment_count(&chain), 0);
}

#[test]
fn an_insert_return_never_stands_in_for_an_unchecked_input() {
    let chain = chain(
        vec![insert()],
        EndpointDisables {
            inputs: vec![off("in")],
            ..EndpointDisables::default()
        },
    );
    assert_eq!(
        segment_count(&chain),
        0,
        "#328: with every input unchecked the chain has no source — feeding the pre-insert blocks from the insert's own return would loop the pedal into itself"
    );
}

#[test]
fn a_chain_without_bindings_keeps_its_fallback_endpoints() {
    let mut legacy = chain(vec![], EndpointDisables::default());
    legacy.io_binding_ids.clear();
    assert_eq!(
        endpoints(&legacy),
        (1, 1),
        "the pre-#328 fallback stays for a chain that selects no E/S"
    );
}
