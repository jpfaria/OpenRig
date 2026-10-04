use super::*;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoEndpoint};

fn chain_with(binding_ids: &[&str]) -> Chain {
    Chain {
        id: ChainId("di771".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: binding_ids.iter().map(|s| s.to_string()).collect(),
        blocks: vec![],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

fn out(name: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

fn registry_two_outputs() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![out("out_main", vec![0, 1]), out("out_fx", vec![2, 3])],
    }]
}

#[test]
fn none_resolves_to_first_output() {
    let chain = chain_with(&["io"]);
    let out = resolve_isolated_output(&chain, &registry_two_outputs(), None).unwrap();
    assert_eq!(out.chain_index, 0);
    assert_eq!(out.entry.channels, vec![0, 1]);
}

#[test]
fn named_endpoint_resolves_to_its_flat_index() {
    let chain = chain_with(&["io"]);
    let r = Some(("io", "out_fx"));
    let out = resolve_isolated_output(&chain, &registry_two_outputs(), r).unwrap();
    assert_eq!(out.chain_index, 1);
    assert_eq!(out.entry.channels, vec![2, 3]);
}

#[test]
fn second_binding_endpoint_gets_a_flat_index_past_the_first_binding() {
    let chain = chain_with(&["io", "io2"]);
    let mut registry = registry_two_outputs();
    registry.push(IoBinding {
        id: "io2".into(),
        name: "IO2".into(),
        inputs: vec![],
        outputs: vec![out("mon", vec![0, 1])],
    });
    let r = Some(("io2", "mon"));
    assert_eq!(
        resolve_isolated_output(&chain, &registry, r)
            .unwrap()
            .chain_index,
        2
    );
}

#[test]
fn stale_ref_falls_back_to_first_output() {
    let chain = chain_with(&["io"]);
    let r = Some(("gone", "x"));
    let out = resolve_isolated_output(&chain, &registry_two_outputs(), r).unwrap();
    assert_eq!(out.chain_index, 0);
    assert_eq!(out.entry.channels, vec![0, 1]);
}

#[test]
fn an_output_outside_the_chain_plays_on_its_own_device_channels() {
    let chain = chain_with(&["io"]);
    let mut registry = registry_two_outputs();
    registry.push(IoBinding {
        id: "other".into(),
        name: "Other".into(),
        inputs: vec![],
        outputs: vec![IoEndpoint {
            name: "FRFR".into(),
            device_id: DeviceId("dev2".into()),
            mode: ChannelMode::Stereo,
            channels: vec![24, 25],
        }],
    });
    let out = resolve_isolated_output(&chain, &registry, Some(("other", "FRFR"))).unwrap();
    assert_eq!(out.entry.device_id, DeviceId("dev2".into()));
    assert_eq!(out.entry.channels, vec![24, 25]);
}

#[test]
fn an_unbound_chain_with_nothing_saved_plays_nowhere() {
    let chain = chain_with(&[]);
    assert!(resolve_isolated_output(&chain, &registry_two_outputs(), None).is_none());
}
