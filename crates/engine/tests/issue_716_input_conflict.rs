//! #716 model A: two or more ACTIVE inputs may not share the same
//! `(device, channel)` — within a chain AND globally across active chains.
//! Output may be shared (many inputs may feed one output). This pins the pure
//! detector the activation path uses; `None` = safe to activate.

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime_endpoints::{input_conflicting_chains, input_port_conflict, InputEntry};
use project::chain::{Chain, ChainInputMode};

fn input(dev: &str, channels: Vec<usize>) -> InputEntry {
    InputEntry {
        device_id: DeviceId(dev.into()),
        mode: ChainInputMode::Mono,
        channels,
    }
}

#[test]
fn two_inputs_on_same_device_and_channel_conflict() {
    let inputs = vec![input("scarlett", vec![0]), input("scarlett", vec![0])];
    assert_eq!(
        input_port_conflict(&inputs),
        Some(("scarlett".to_string(), 0)),
        "two inputs reading device 'scarlett' channel 0 at once is a conflict"
    );
}

#[test]
fn same_device_different_channels_is_not_a_conflict() {
    // The "two E/S on one device" case the user explicitly allowed.
    let inputs = vec![input("scarlett", vec![0]), input("scarlett", vec![1])];
    assert_eq!(input_port_conflict(&inputs), None);
}

#[test]
fn different_devices_same_channel_is_not_a_conflict() {
    let inputs = vec![input("a", vec![0]), input("b", vec![0])];
    assert_eq!(input_port_conflict(&inputs), None);
}

#[test]
fn multi_channel_input_overlapping_another_conflicts() {
    // Within one chain: a stereo input [0,1] plus a mono input [1] collide on 1.
    let inputs = vec![input("dev", vec![0, 1]), input("dev", vec![1])];
    assert_eq!(input_port_conflict(&inputs), Some(("dev".to_string(), 1)));
}

fn chain(id: &str, binding: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![binding.into()],
        blocks: vec![],
    }
}

fn binding(id: &str, dev: &str, in_ch: usize) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId(dev.into()),
            mode: ChannelMode::Mono,
            channels: vec![in_ch],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId(dev.into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }
}

#[test]
fn second_chain_sharing_an_input_tap_is_skipped_first_wins() {
    // a and b both select bindings that read dev "scarlett" channel 0.
    let registry = vec![binding("io_a", "scarlett", 0), binding("io_b", "scarlett", 0)];
    let chains = vec![chain("a", "io_a"), chain("b", "io_b")];
    let skip = input_conflicting_chains(chains.iter(), &registry);
    assert_eq!(skip, vec![ChainId("b".into())], "first chain wins; the second is skipped");
}

#[test]
fn distinct_input_channels_activate_both() {
    let registry = vec![binding("io_a", "scarlett", 0), binding("io_b", "scarlett", 1)];
    let chains = vec![chain("a", "io_a"), chain("b", "io_b")];
    assert!(input_conflicting_chains(chains.iter(), &registry).is_empty());
}
