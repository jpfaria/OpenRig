//! The stream builder records on the runtime it returns the chain STRUCTURE
//! its streams were built for (`ActiveChainRuntime::structure`, #881).
//! `chain_structure_changed` compares that record with the chain the caller
//! now holds, to tell a structural edit (new streams) from a param edit (DSP
//! only). If the record is not the chain's signature against the SAME binding
//! registry, edits are misclassified.
//!
//! No device is opened. The resolved config has no inputs and no outputs, so
//! the cpal path builds zero streams. On linux+JACK the no-server branch
//! returns before any client exists.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

use crate::io_topology::chain_structure_signature;
use crate::resolved::{ChainStreamSignature, ResolvedChainAudioConfig};

/// Two E/S on two devices plus an insert loop. With the insert off the chain
/// splits into one runtime per head, so its structure depends on the registry.
fn registry() -> Vec<IoBinding> {
    let ep = |name: &str, dev: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    vec![
        IoBinding {
            id: "scarlett".into(),
            name: "SCARLETT".into(),
            inputs: vec![ep("in", "scarlett", 0)],
            outputs: vec![ep("out", "scarlett", 0)],
        },
        IoBinding {
            id: "teyun".into(),
            name: "TEYUN".into(),
            inputs: vec![ep("in", "teyun", 0)],
            outputs: vec![ep("out", "teyun", 0)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![ep("ret", "scarlett", 3)],
            outputs: vec![ep("snd", "scarlett", 3)],
        },
    ]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-2".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett".into(), "teyun".into()],
        blocks: vec![AudioBlock {
            id: BlockId("insert".into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

/// A resolved config with no device at all: the builder has nothing to open.
fn resolved_without_devices() -> ResolvedChainAudioConfig {
    ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate: 48_000.0,
        by_device: std::collections::HashMap::new(),
        output_devices_by_input_cpal: Vec::new(),
        stream_signature: ChainStreamSignature {
            inputs: Vec::new(),
            outputs: Vec::new(),
        },
    }
}

#[test]
fn the_built_runtime_records_the_chain_structure_against_the_binding_registry() {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if crate::host::jack_server_is_running() {
            eprintln!(
                "skipped: a JACK server is running; this test pins the no-device path \
                 and must never open a JACK client"
            );
            return;
        }
    }

    let chain = chain();
    let registry = registry();

    let active = super::build_active_chain_runtime(
        &chain.id,
        &chain,
        resolved_without_devices(),
        Vec::new(),
        &registry,
        &[],
        1,
    )
    .expect("a chain with no resolved device builds without opening a stream");

    assert!(
        active._input_streams.is_empty() && active._output_streams.is_empty(),
        "no device stream may be opened by this test"
    );
    assert_eq!(
        active.structure,
        chain_structure_signature(&chain, &registry),
        "#881: the runtime must record the chain's structure against the SAME \
         binding registry, or chain_structure_changed misreads every later edit"
    );
    assert_ne!(
        active.structure,
        chain_structure_signature(&chain, &[]),
        "fixture: this chain's structure depends on the registry (two heads with \
         the insert off = two runtimes), so a signature taken without it is wrong"
    );
}
