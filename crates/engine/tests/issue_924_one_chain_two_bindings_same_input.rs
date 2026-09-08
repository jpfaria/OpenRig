//! #924 — one chain may read ONE capture point through several of its own E/S.
//!
//! The owner's rig: one chain bound to GUITARRA 1 MAIN and GUITARRA 1 SYN5050,
//! both reading input ch 0 of the same interface and feeding different outputs —
//! two isolated pipelines from one guitar. The per-chain enable guard (#833)
//! let it play, but the project-wide sync flagged the chain as conflicting with
//! ITSELF and skipped it, so every I/O edit that rebuilt the project left the
//! rig silent until it was reopened. The input-conflict rule is a rule between
//! chains: the backend hands the same capture point to both pipelines.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime_endpoints::{conflicting_input_channel, input_conflicting_chains};
use project::chain::Chain;

fn ep(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("coreaudio:quantum".into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn binding(id: &str, input: &[usize], out: &[usize]) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![ep("in", ChannelMode::Mono, input)],
        outputs: vec![ep("out", ChannelMode::Stereo, out)],
    }
}

fn chain(id: &str, bindings: &[&str]) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: bindings.iter().map(|b| b.to_string()).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn owner_registry() -> Vec<IoBinding> {
    vec![
        binding("guitarra-1", &[0], &[0, 1]),
        binding("guitarra-1-5050", &[0], &[16, 17]),
    ]
}

#[test]
fn a_chain_reading_one_input_through_two_of_its_own_bindings_is_not_skipped() {
    let registry = owner_registry();
    let chains = vec![chain("rig:input-2", &["guitarra-1", "guitarra-1-5050"])];

    assert!(
        conflicting_input_channel(&chains[0], chains.iter(), &registry).is_none(),
        "the enable guard lets this chain play"
    );
    assert!(
        input_conflicting_chains(chains.iter(), &registry).is_empty(),
        "#924: the project-wide sync must not flag the chain as conflicting with itself"
    );
}

#[test]
fn another_chain_on_that_same_input_is_still_skipped() {
    let mut registry = owner_registry();
    registry.push(binding("guitarra-2", &[0], &[2, 3]));
    let chains = vec![
        chain("rig:input-2", &["guitarra-1", "guitarra-1-5050"]),
        chain("rig:input-3", &["guitarra-2"]),
    ];

    assert_eq!(
        input_conflicting_chains(chains.iter(), &registry),
        vec![ChainId("rig:input-3".into())],
        "the rule between chains stays: the second chain on ch 0 is skipped"
    );
}
