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
    assert_eq!(
        resolve_di_output_index(&chain, &registry_two_outputs(), None),
        0
    );
}

#[test]
fn named_endpoint_resolves_to_its_flat_index() {
    let chain = chain_with(&["io"]);
    let r = DiOutputRef {
        binding_id: "io".into(),
        endpoint: "out_fx".into(),
    };
    assert_eq!(
        resolve_di_output_index(&chain, &registry_two_outputs(), Some(&r)),
        1
    );
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
    let r = DiOutputRef {
        binding_id: "io2".into(),
        endpoint: "mon".into(),
    };
    assert_eq!(resolve_di_output_index(&chain, &registry, Some(&r)), 2);
}

#[test]
fn stale_ref_falls_back_to_first_output() {
    let chain = chain_with(&["io"]);
    let r = DiOutputRef {
        binding_id: "gone".into(),
        endpoint: "x".into(),
    };
    assert_eq!(
        resolve_di_output_index(&chain, &registry_two_outputs(), Some(&r)),
        0
    );
}
