//! #85 — the user's chain runs on the SCARLET and carries a mid `Output` that
//! writes to the TEYUN. Nothing comes out of the TEYUN.
//!
//! An output device's stream only mixes the runtimes that
//! `output_devices_by_input_cpal` says feed it (stream-isolation LAW). That map
//! is built per binding GROUP, and the mid output's binding contributes no
//! INPUT to this chain — so its device never reaches the chain's input index,
//! the runtime is filtered out of that device's stream, and the tap is silent.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, OutputBlock};
use project::chain::Chain;

use super::chain_resolve::output_devices_by_input_cpal;

const SCARLETT: &str = "coreaudio:scarlett";
const TEYUN: &str = "coreaudio:teyun";

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

/// Two interfaces, one E/S each — the shape of the user's rig.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "scarlett".into(),
            name: "SCARLET - ALL".into(),
            inputs: vec![endpoint("In 1", SCARLETT, vec![0])],
            outputs: vec![endpoint("Out 1", SCARLETT, vec![0, 1])],
        },
        IoBinding {
            id: "teyun".into(),
            name: "TEYUN - ALL".into(),
            inputs: vec![endpoint("In 1", TEYUN, vec![0])],
            outputs: vec![endpoint("Out 1", TEYUN, vec![0, 1])],
        },
    ]
}

/// The chain runs on the SCARLETT and taps the TEYUN mid-chain.
fn chain_with_mid_output() -> Chain {
    Chain {
        id: ChainId("rig:input-2".into()),
        description: Some("GUITARRA - TONES".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett".into()],
        blocks: vec![AudioBlock {
            id: BlockId("rig:input-2:output:1".into()),
            enabled: true,
            kind: AudioBlockKind::Output(OutputBlock {
                model: "standard".into(),
                io: "teyun".into(),
                endpoint: "Out 1".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

fn map_for(chain: &Chain) -> Vec<Vec<String>> {
    let registry = registry();
    let (logical_inputs, _) = engine::runtime_endpoints::resolve_chain_io(chain, &registry);
    output_devices_by_input_cpal(chain, &registry, &logical_inputs)
}

#[test]
fn the_mid_outputs_device_is_fed_by_the_chains_input_stream() {
    let map = map_for(&chain_with_mid_output());

    assert!(
        map[0].iter().any(|d| d == TEYUN),
        "#85: the chain's runtime must feed the TEYUN stream — the mid Output \
         writes there. Got {map:?}, so that stream drops this runtime and the \
         port is silent"
    );
}

#[test]
fn the_chains_own_output_device_is_still_fed() {
    let map = map_for(&chain_with_mid_output());

    assert!(
        map[0].iter().any(|d| d == SCARLETT),
        "the chain's own tail output must keep its device (got {map:?})"
    );
}

/// Isolation is not widened: a chain with no mid port still feeds only its own
/// binding's output device, never another interface's.
#[test]
fn a_chain_without_a_mid_port_feeds_only_its_own_device() {
    let mut chain = chain_with_mid_output();
    chain.blocks.clear();
    let map = map_for(&chain);

    assert_eq!(map[0], vec![SCARLETT.to_string()]);
}
