//! #979 — a route born with a cushion several buffers deep: the owner's
//! decision (option 2, part a) is that one late push never teaches the #953
//! drift guard a level below what the route needs. A route fed by a cab is
//! born resting at its whole cushion (#592/#965): that cushion is what it
//! needs.
//!
//! Two such routes, sized by the engine from what the stream layer hands it:
//! - the JACK sizing (x8: 512 frames at a 64-frame buffer) on the chain's own
//!   clock;
//! - a cab route on another device than its producer, which keeps the #592
//!   512-frame cushion.
//!
//! One push one period late costs such a route nothing — its cushion absorbs
//! it. The guard used to take the dip that push left as the route's level,
//! and once the worker was back on time it cut a buffer of live audio (a
//! counted trim) and left the route one buffer shallower; every later late
//! push ratcheted it down again, until lateness the cushion was sized for
//! cost gaps a fresh build never pays.

use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{
    build_chain_runtime_state, process_input_f32, process_output_f32, ChainRuntimeState,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const FRAMES: usize = 64;
/// Callbacks per #953 guard window: 8192 frames at 64 frames.
const WINDOW: usize = 8_192 / FRAMES;
/// On-time cycles before anything is measured.
const WARM_UP: usize = 8 * WINDOW;
/// On-time cycles after a late push: a guard window to close on the dip, and
/// a few more for any cut it would make.
const AFTER: usize = 4 * WINDOW + 7;
/// Late pushes, one after another, each at a different window phase.
const EVENTS: usize = 12;

fn endpoint(device: &str) -> IoEndpoint {
    IoEndpoint {
        name: device.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Mono,
        channels: vec![0],
    }
}

fn registry(input_device: &str, output_device: &str) -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![endpoint(input_device)],
        outputs: vec![endpoint(output_device)],
    }]
}

fn cab_chain() -> Chain {
    Chain {
        id: ChainId("chain:979:deep-cushion".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![AudioBlock {
            id: BlockId("cab".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: block_core::EFFECT_TYPE_CAB.into(),
                model: "ir_test_fake_cel_cream_4x12".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
        disabled_endpoints: Default::default(),
        mix: Default::default(),
    }
}

struct Route {
    runtime: Arc<ChainRuntimeState>,
    input: Vec<f32>,
    out: Vec<f32>,
    /// Buffers the worker holds and has not pushed.
    held: usize,
}

impl Route {
    fn new(target: usize, input_device: &str, output_device: &str) -> Self {
        let runtime = build_chain_runtime_state(
            &cab_chain(),
            SR,
            &[target],
            &registry(input_device, output_device),
        )
        .expect("a cab chain builds");
        Self {
            runtime: Arc::new(runtime),
            input: vec![0.1; FRAMES],
            out: vec![0.0; FRAMES],
            held: 0,
        }
    }

    /// (frames queued at the callback start, underrun frames, trims).
    fn stats(&self) -> (usize, u64, u64) {
        let stats = self
            .runtime
            .take_output_route_stats()
            .into_iter()
            .next()
            .expect("the chain owns one output route");
        (stats.fill_frames, stats.underruns, stats.latency_trims)
    }

    /// One HAL cycle: the output callback, then the worker hands over what it
    /// holds unless it is late. Returns the fill the callback started with.
    fn cycle(&mut self, late: bool) -> usize {
        self.held += 1;
        let fill = self.stats().0;
        process_output_f32(&self.runtime, 0, &mut self.out, 1);
        if !late {
            while self.held > 0 {
                process_input_f32(&self.runtime, 0, &self.input, 1);
                self.held -= 1;
            }
        }
        fill
    }

    fn run(&mut self, cycles: usize) -> usize {
        let mut fill = 0;
        for _ in 0..cycles {
            fill = self.cycle(false);
        }
        fill
    }
}

fn one_late_push_at_a_time(label: &str, mut route: Route) {
    let rest = route.run(WARM_UP);
    let (_, underruns, trims) = route.stats();
    for event in 0..EVENTS {
        route.cycle(true);
        let fill = route.run(AFTER + event);
        let (_, now_underruns, now_trims) = route.stats();
        assert_eq!(
            (now_underruns - underruns, now_trims - trims, fill),
            (0, 0, rest),
            "{label}, late push {event}: (underrun frames, trims, frames queued at a callback \
             start) — one push one period late taught the guard a level below the route's \
             {rest}-frame cushion: it cut live audio once the worker was back on time and \
             left the route shallower"
        );
    }
}

#[test]
fn a_route_sized_eight_buffers_deep_keeps_its_cushion_through_late_pushes() {
    one_late_push_at_a_time(
        "cab route sized x8 (512 frames) on its producer's clock",
        Route::new(8 * FRAMES, "dev", "dev"),
    );
}

#[test]
fn a_cab_route_on_another_clock_keeps_its_592_cushion_through_late_pushes() {
    one_late_push_at_a_time(
        "cab route on another device (the #592 cushion)",
        Route::new(2 * FRAMES, "dev-in", "dev-out"),
    );
}
