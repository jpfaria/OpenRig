//! #980 — `openrig://routes` must say WHERE the audio a route missed went.
//!
//! Owner's rig: two guitars on one Quantum HD 8, a NAM + IR + 2 VST3 chain,
//! the xrun LED blinking. The routes read showed underruns rising on every
//! route with `fill_frames` steady and `latency_trims: 0` — which fits both
//! "the dsp-worker ran late and caught up" and "the buffer was never pushed".
//! Nobody could tell them apart because two losses were silent:
//!
//! - a full ring discarding what a late producer pushes when it catches up;
//! - `process_input_f32` dropping a whole input buffer when another thread
//!   holds the runtime's `processing` lock (a lost `try_lock`).
//!
//! Both are counted now, so the next reading names the cause.

use std::collections::HashMap;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use super::{process_input_f32, process_output_f32};
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_graph::build_per_input_runtimes;
use crate::runtime_state::ChainRuntimeState;

const DEVICE: &str = "coreaudio:quantum";
const DEVICE_CHANNELS: usize = 12;
const FRAMES: usize = 64;

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEVICE.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn runtime() -> Arc<ChainRuntimeState> {
    let registry = vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Guitarra 1".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, &[0])],
        outputs: vec![endpoint("main", ChannelMode::Stereo, &[0, 1])],
    }];
    let chain = Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![AudioBlock {
            id: BlockId("gain".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let runtimes = build_per_input_runtimes(
        &chain,
        44_100.0,
        &HashMap::new(),
        &vec![DEFAULT_ELASTIC_TARGET; 1],
        &registry,
    )
    .expect("the chain must build");
    Arc::new(runtimes.into_iter().next().expect("one runtime").1)
}

fn input(runtime: &Arc<ChainRuntimeState>) {
    let data = vec![0.1_f32; FRAMES * DEVICE_CHANNELS];
    process_input_f32(runtime, 0, &data, DEVICE_CHANNELS);
}

fn output(runtime: &Arc<ChainRuntimeState>) {
    let mut out = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    process_output_f32(runtime, 0, &mut out, DEVICE_CHANNELS);
}

/// A producer that falls behind and then catches up pushes more than the ring
/// holds. Every frame is accounted for: primed + pushed = popped + queued +
/// dropped — so the frames the late producer lost show up as `dropped_frames`.
#[test]
fn frames_a_late_producer_loses_on_a_full_route_are_counted() {
    let runtime = runtime();
    let primed = runtime.take_output_route_stats()[0].fill_frames as u64;

    let (mut inputs, mut outputs) = (0_u64, 0_u64);
    for _ in 0..20 {
        input(&runtime);
        output(&runtime);
        inputs += 1;
        outputs += 1;
    }
    // The output stream keeps running while the producer stalls...
    for _ in 0..6 {
        output(&runtime);
        outputs += 1;
    }
    // ...then the producer catches up in one burst.
    for _ in 0..12 {
        input(&runtime);
        inputs += 1;
    }

    let route = &runtime.take_output_route_stats()[0];
    let pushed = inputs * FRAMES as u64;
    let popped = outputs * FRAMES as u64 - route.underruns;
    let queued = route.fill_frames as u64;
    assert!(
        primed + pushed > popped + queued,
        "precondition: the catch-up must overflow the ring (primed {primed}, pushed {pushed}, popped {popped}, queued {queued})"
    );
    assert_eq!(
        route.dropped_frames,
        primed + pushed - popped - queued,
        "every frame the full ring discarded must be counted"
    );
}

/// Another thread holding `processing` makes the input callback drop its
/// whole buffer — a silent period on every route of the runtime. It must be
/// counted, not vanish.
#[test]
fn an_input_buffer_lost_to_a_held_processing_lock_is_counted() {
    let runtime = runtime();
    {
        let _held = runtime.processing.lock().expect("lock");
        input(&runtime);
    }
    input(&runtime);

    let route = &runtime.take_output_route_stats()[0];
    assert_eq!(
        route.input_busy_skips, 1,
        "exactly the buffer that met the held lock is lost"
    );
}
