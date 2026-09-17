//! #947: a runtime owns an output route ONLY for an output its own segments
//! write. A chain on two bindings builds one runtime per binding; each one used
//! to also get a route for the OTHER binding's output — a route no one feeds,
//! popped empty on every frame by the output stream (millions of underruns,
//! the overload LED lit on start). Pinned on the initial build AND on a live
//! rebuild, the two places routes are made.

use std::collections::{BTreeSet, HashMap};

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

use crate::runtime_graph::build_per_input_runtimes;
use crate::runtime_graph_update::update_chain_runtime_state;
use crate::runtime_state::ChainRuntimeState;

const RATE: f32 = 48_000.0;

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

fn two_guitar_chain() -> Chain {
    Chain {
        id: ChainId("rig:input-4".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitar-1".into(), "guitar-2".into()],
        blocks: Vec::new(),
        di_output: None,
        loopers: vec![],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![
        binding("guitar-1", vec![0, 1]),
        binding("guitar-2", vec![2, 3]),
    ]
}

fn written_routes(runtime: &ChainRuntimeState) -> BTreeSet<usize> {
    let processing = runtime.processing.lock().expect("processing lock");
    processing
        .input_states
        .iter()
        .flat_map(|state| {
            state
                .output_route_indices
                .iter()
                .copied()
                .chain(state.mid_output_taps.iter().map(|tap| tap.route_idx))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn owned_routes(runtime: &ChainRuntimeState) -> BTreeSet<usize> {
    runtime
        .take_output_route_stats()
        .into_iter()
        .map(|stats| stats.route)
        .collect()
}

fn assert_each_runtime_owns_only_what_it_writes(
    runtimes: &[(usize, &ChainRuntimeState)],
    when: &str,
) {
    let mut every_output = BTreeSet::new();
    for (group, runtime) in runtimes {
        let written = written_routes(runtime);
        assert_eq!(
            written.len(),
            1,
            "{when}: guitar {group} writes its own output only"
        );
        assert_eq!(
            owned_routes(runtime),
            written,
            "{when}: guitar {group} must own a route for its own output only — \
             never one for the other guitar's output"
        );
        every_output.extend(written);
    }
    assert_eq!(
        every_output.len(),
        2,
        "{when}: each guitar has its own output"
    );
}

#[test]
fn each_guitar_runtime_owns_only_its_own_output_route() {
    let chain = two_guitar_chain();
    let registry = registry();
    let runtimes: Vec<(usize, std::sync::Arc<ChainRuntimeState>)> =
        build_per_input_runtimes(&chain, RATE, &HashMap::new(), &[], &registry)
            .expect("two-binding chain builds")
            .into_iter()
            .map(|(group, state)| (group, std::sync::Arc::new(state)))
            .collect();
    assert_eq!(runtimes.len(), 2, "one runtime per guitar");

    let view: Vec<(usize, &ChainRuntimeState)> =
        runtimes.iter().map(|(g, r)| (*g, r.as_ref())).collect();
    assert_each_runtime_owns_only_what_it_writes(&view, "initial build");

    for (_, runtime) in &runtimes {
        update_chain_runtime_state(runtime, &chain, RATE, false, &[], &registry)
            .expect("live rebuild");
    }
    assert_each_runtime_owns_only_what_it_writes(&view, "live rebuild");
}
