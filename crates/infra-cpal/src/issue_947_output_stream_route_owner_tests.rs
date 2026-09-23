//! #947: an output stream mixes ONLY the runtimes that write ITS output. Two
//! guitars on two bindings whose outputs sit on one interface are two runtimes
//! on one device — but guitar 2's runtime writes nothing to guitar 1's output,
//! so guitar 1's output stream must not hold guitar 2's runtime at all.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::build_per_input_runtime_states;
use project::chain::Chain;

use super::*;

fn binding(id: &str, out_channels: Vec<usize>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "In".into(),
            device_id: DeviceId(format!("{id}-in")),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "Out".into(),
            device_id: DeviceId("shared-out".into()),
            mode: ChannelMode::Stereo,
            channels: out_channels,
        }],
    }
}

#[test]
fn each_guitar_output_stream_holds_only_its_own_guitar_runtime() {
    let registry = vec![
        binding("guitar-1", vec![0, 1]),
        binding("guitar-2", vec![2, 3]),
    ];
    let chain = Chain {
        id: ChainId("rig:input-4".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitar-1".into(), "guitar-2".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    let runtimes =
        build_per_input_runtime_states(&chain, 48_000.0, &HashMap::new(), &[], &registry)
            .expect("two-binding chain builds");
    let slots = build_chain_slots(&runtimes);
    let map: Vec<Vec<String>> = runtimes.iter().map(|_| vec!["shared-out".into()]).collect();

    for (output_index, (_, own)) in runtimes.iter().enumerate() {
        let mixed = slots_for_output_stream(&slots, &map, "shared-out", output_index);
        assert_eq!(
            mixed.len(),
            1,
            "output {output_index} must hold ONLY the guitar that writes it, not the other guitar"
        );
        assert!(
            Arc::ptr_eq(&mixed[0].load(), own),
            "output {output_index} holds the wrong guitar's runtime"
        );
    }
}

/// #967: a bound insert's send stream is opened with the chain even while the
/// insert is OFF — and it must hold the chain's runtime then too. Its route is
/// unwritten while the loop is off (the stream plays silence), but the switch
/// ON is a DSP rebuild published into the SAME slot: a send stream built with no
/// slot would stay silent after it, the gear would get nothing and everything
/// after the insert would go quiet.
#[test]
fn a_disabled_inserts_send_stream_holds_the_chain_runtime() {
    use project::block::{AudioBlock, AudioBlockKind, InsertBlock};

    let ep = |name: &str, dev: &str, channels: Vec<usize>| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels,
    };
    let registry = vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![ep("in", "hd8", vec![0])],
            outputs: vec![ep("out", "hd8", vec![0])],
        },
        IoBinding {
            id: "fx".into(),
            name: "SYN-2".into(),
            inputs: vec![ep("ret", "hd8", vec![17])],
            outputs: vec![ep("snd", "hd8", vec![10])],
        },
    ];
    let chain = Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("syn2".into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let runtimes =
        build_per_input_runtime_states(&chain, 48_000.0, &HashMap::new(), &[], &registry)
            .expect("the chain builds");
    let slots = build_chain_slots(&runtimes);
    let map: Vec<Vec<String>> = runtimes.iter().map(|_| vec!["hd8".into()]).collect();

    let send_route = 1;
    assert_eq!(
        slots_for_output_stream(&slots, &map, "hd8", send_route).len(),
        1,
        "#967: the switched-off loop's send stream must hold the chain's runtime"
    );
}
