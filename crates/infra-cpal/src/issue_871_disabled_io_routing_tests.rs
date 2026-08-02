//! #871 — the routing consequence of a DISABLED mid `Input`/`Output` block.
//!
//! The owner's `GUITARRA - TONES` runs on the SCARLETT's input 1 and the
//! TEYUN's input 1, and carries a mid `Input` bound to the SCARLETT's channel 1
//! (physical input **2**) plus a mid `Output` bound to the TEYUN — both toggled
//! OFF. Guitar plugged into the Scarlett's input 2 was audible, and it came out
//! of the TEYUN: the disabled input still opened its stream and the mid-input
//! pass still routed it to the chain's tail devices.

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime_endpoints::resolve_chain_io;
use project::block::{AudioBlock, AudioBlockKind, InputBlock, OutputBlock};
use project::chain::Chain;

use super::chain_resolve_io_map::output_devices_by_input_cpal;

const SCARLETT: &str = "coreaudio:scarlett";
const TEYUN: &str = "coreaudio:teyun";

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels,
    }
}

fn stereo(name: &str, device: &str) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels: vec![0, 1],
    }
}

/// The owner's registry: one E/S per input channel plus the "ALL" TEYUN E/S.
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "scarlett-1".into(),
            name: "SCARLET - 1".into(),
            inputs: vec![endpoint("In 1", SCARLETT, vec![0])],
            outputs: vec![stereo("Out 1", SCARLETT)],
        },
        IoBinding {
            id: "teyun-1".into(),
            name: "TEYUN - 1".into(),
            inputs: vec![endpoint("In 1", TEYUN, vec![0])],
            outputs: vec![stereo("Out 1", TEYUN)],
        },
        IoBinding {
            id: "scarlett-2".into(),
            name: "SCARLET - 2".into(),
            inputs: vec![endpoint("In 1", SCARLETT, vec![1])],
            outputs: vec![stereo("Out 1", SCARLETT)],
        },
        IoBinding {
            id: "teyun-all".into(),
            name: "TEYUN - ALL".into(),
            inputs: vec![endpoint("In 1", TEYUN, vec![0])],
            outputs: vec![stereo("Out 1", TEYUN)],
        },
    ]
}

fn disabled_mid_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:tones:input:1".into()),
        enabled: false,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "scarlett-2".into(),
            endpoint: "In 1".into(),
        }),
    }
}

fn disabled_mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:tones:output:1".into()),
        enabled: false,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "teyun-all".into(),
            endpoint: "Out 1".into(),
        }),
    }
}

fn owners_chain() -> Chain {
    Chain {
        id: ChainId("rig:tones".into()),
        description: Some("GUITARRA - TONES".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["scarlett-1".into(), "teyun-1".into()],
        blocks: vec![disabled_mid_input(), disabled_mid_output()],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn a_disabled_mid_input_opens_no_stream() {
    let (inputs, _) = resolve_chain_io(&owners_chain(), &registry());
    assert_eq!(
        inputs.len(),
        2,
        "only the two head inputs are live, not the disabled block's"
    );
    assert!(
        !inputs
            .iter()
            .any(|i| i.device_id.0 == SCARLETT && i.channels == vec![1]),
        "the Scarlett's channel 1 belongs to a disabled Input — no stream for it"
    );
}

#[test]
fn a_disabled_mid_output_claims_no_device_route() {
    let chain = owners_chain();
    let registry = registry();
    let (inputs, _) = resolve_chain_io(&chain, &registry);
    let by_cpal = output_devices_by_input_cpal(&chain, &registry, &inputs);

    // cpal 0 = SCARLETT (head In 1), cpal 1 = TEYUN (head In 1).
    assert_eq!(by_cpal.len(), 2, "two input devices → two cpal indices");
    assert_eq!(
        by_cpal[0],
        vec![SCARLETT.to_string()],
        "the Scarlett stream writes ONLY the Scarlett's output — the TEYUN route \
         belongs to a DISABLED Output block (stream-isolation LAW)"
    );
    assert_eq!(
        by_cpal[1],
        vec![TEYUN.to_string()],
        "the TEYUN stream writes ONLY the TEYUN's output"
    );
}
