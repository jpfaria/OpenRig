//! #967 — a route's sample-rate converter lives exactly as long as the route
//! it feeds. An in-place update that keeps the route (the same `Arc`) keeps
//! the converter's phase and history; one that builds a fresh route at the
//! same index must start it with no converter, or samples from before the
//! edit are played in front of the new route.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

use crate::runtime::process_input_f32;
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_state::{ChainRuntimeState, OutputRoutingState};

const CHAIN_RATE: f32 = 44_100.0;
const DEVICE_RATE: f32 = 48_000.0;
const FRAMES: usize = 128;
const CHANNELS: usize = 2;
const ROUTE: usize = 0;

/// One E/S whose input is on the interface that clocks the chain (44.1 kHz)
/// and whose output is on another one (48 kHz): route 0 is cross-rate.
fn registry() -> Vec<IoBinding> {
    let ep = |dev: &str| IoEndpoint {
        name: "port".into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    };
    vec![IoBinding {
        id: "a".into(),
        name: "A".into(),
        inputs: vec![ep("scarlett")],
        outputs: vec![ep("teyun")],
    }]
}

fn device_rates() -> HashMap<DeviceId, f32> {
    HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE),
        (DeviceId("teyun".into()), DEVICE_RATE),
    ])
}

fn chain(volume: f32) -> Chain {
    Chain {
        id: ChainId("cross-rate".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume,
        io_binding_ids: vec!["a".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    }
}

fn build(chain: &Chain) -> Arc<ChainRuntimeState> {
    Arc::new(
        crate::runtime_graph::build_chain_runtime_state_with_device_rates(
            chain,
            CHAIN_RATE,
            &device_rates(),
            &[DEFAULT_ELASTIC_TARGET],
            &registry(),
        )
        .expect("the cross-rate chain must build"),
    )
}

fn play(runtime: &Arc<ChainRuntimeState>) {
    let input = vec![0.3_f32; FRAMES * CHANNELS];
    for _ in 0..4 {
        process_input_f32(runtime, 0, &input, CHANNELS);
    }
}

fn update(runtime: &Arc<ChainRuntimeState>, chain: &Chain, reset_output_queue: bool) {
    super::update_chain_runtime_state_at_device_rates(
        runtime,
        chain,
        &device_rates(),
        reset_output_queue,
        &[DEFAULT_ELASTIC_TARGET],
        &registry(),
    )
    .expect("the in-place update applies");
}

fn route(runtime: &ChainRuntimeState, idx: usize) -> Arc<OutputRoutingState> {
    Arc::clone(
        runtime.output_routes.load()[idx]
            .as_ref()
            .expect("the cross-rate output is written"),
    )
}

fn has_resampler(runtime: &ChainRuntimeState, idx: usize) -> bool {
    runtime
        .processing
        .lock()
        .expect("processing lock")
        .input_scratches
        .iter()
        .any(|scratch| matches!(scratch.route_resamplers.get(&idx), Some(Some(_))))
}

#[test]
fn an_update_that_keeps_a_cross_rate_route_keeps_its_resampler() {
    let runtime = build(&chain(100.0));
    play(&runtime);
    assert!(
        has_resampler(&runtime, ROUTE),
        "precondition: feeding the 44.1 kHz -> 48 kHz route gave it a converter"
    );
    let before = route(&runtime, ROUTE);

    update(&runtime, &chain(80.0), false);

    assert!(
        Arc::ptr_eq(&before, &route(&runtime, ROUTE)),
        "precondition: a volume edit keeps the route itself"
    );
    assert!(
        has_resampler(&runtime, ROUTE),
        "#967: the route survived the update but its converter was thrown away — \
         the next callback restarts it from silence mid-stream"
    );
}

#[test]
fn a_cross_rate_route_rebuilt_at_the_same_index_drops_its_resampler() {
    let runtime = build(&chain(100.0));
    play(&runtime);
    assert!(
        has_resampler(&runtime, ROUTE),
        "precondition: feeding the 44.1 kHz -> 48 kHz route gave it a converter"
    );
    let before = route(&runtime, ROUTE);

    update(&runtime, &chain(100.0), true);

    assert!(
        !Arc::ptr_eq(&before, &route(&runtime, ROUTE)),
        "precondition: a queue reset builds route 0 fresh at the same index"
    );
    assert!(
        !has_resampler(&runtime, ROUTE),
        "#967: the fresh route inherited the old route's converter history"
    );
}
