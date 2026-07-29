//! #323 — resolving a looper's chosen input endpoint to its segment index.

use super::*;
use crate::chain::{Chain, EndpointRef};
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

fn two_input_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![
            IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            },
            IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("c".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

#[test]
fn none_resolves_to_the_first_input() {
    assert_eq!(
        resolve_input_segment(&chain(), &two_input_binding(), None),
        0
    );
}

#[test]
fn the_second_input_endpoint_resolves_to_segment_1() {
    let target = EndpointRef {
        binding_id: "io".into(),
        endpoint: "in1".into(),
    };
    assert_eq!(
        resolve_input_segment(&chain(), &two_input_binding(), Some(&target)),
        1
    );
}

#[test]
fn a_stale_endpoint_falls_back_to_the_first_input() {
    let target = EndpointRef {
        binding_id: "io".into(),
        endpoint: "gone".into(),
    };
    assert_eq!(
        resolve_input_segment(&chain(), &two_input_binding(), Some(&target)),
        0
    );
}
