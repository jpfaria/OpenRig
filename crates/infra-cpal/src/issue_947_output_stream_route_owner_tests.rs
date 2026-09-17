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
