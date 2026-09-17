//! #947: a chain on TWO bindings builds one runtime per binding, and each
//! runtime carries a route for EVERY output of the chain — but it only feeds
//! its own binding's route. When both outputs sit on one device, that device's
//! stream pops both runtimes on every route, so the route a runtime never feeds
//! is popped empty on every frame: millions of underruns, and the chain's red
//! overload LED lights the moment the chain starts, with nothing overloading.

use std::collections::HashMap;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{build_per_input_runtime_states, process_input_f32, process_output_f32};
use project::chain::Chain;

const RATE: f32 = 48_000.0;
const FRAMES: usize = 256;

fn binding(id: &str, input_dev: &str, out_channels: Vec<usize>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.into(),
        inputs: vec![IoEndpoint {
            name: "In".into(),
            device_id: DeviceId(input_dev.into()),
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
fn a_route_the_runtime_never_feeds_is_not_counted_as_underrun() {
    let registry = vec![
        binding("guitar-1", "guitar-1-in", vec![0, 1]),
        binding("guitar-2", "guitar-2-in", vec![2, 3]),
    ];
    let chain = Chain {
        id: ChainId("rig:input-4".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitar-1".into(), "guitar-2".into()],
        blocks: Vec::new(),
        di_output: None,
        loopers: vec![],
    };
    let runtimes =
        build_per_input_runtime_states(&chain, RATE, &HashMap::new(), &[], &registry).unwrap();
    assert_eq!(runtimes.len(), 2, "one runtime per binding");
    // Each runtime carries a route for both chain outputs (route 0, route 1).
    let routes = 2;

    let input = vec![0.25_f32; FRAMES];
    let mut out = vec![0.0_f32; FRAMES * 4];
    for _ in 0..8 {
        for (_, runtime) in &runtimes {
            let cpal = runtime.input_cpal_index().unwrap_or(0);
            process_input_f32(runtime, cpal, &input, 1);
        }
        // The shared output device's stream pops every runtime on every route.
        for route in 0..routes {
            for (_, runtime) in &runtimes {
                process_output_f32(runtime, route, &mut out, 4);
            }
        }
    }

    let underruns: Vec<u64> = runtimes.iter().map(|(_, r)| r.underrun_count()).collect();
    assert_eq!(
        underruns,
        vec![0, 0],
        "#947: fed routes kept up with their producer, so every underrun here \
         is a route its runtime never feeds — the false overload LED"
    );
}
