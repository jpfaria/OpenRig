//! #953 — after one output stream stalls, its route must not keep the extra
//! latency forever.
//!
//! The owner's rig: one binding, one guitar input, two stereo outputs on the
//! same interface (Main `[0,1]` and ADAT `[24,25]` feeding the FRFR). Both
//! routes run on the device clock, so nothing ever drains a cushion that grew:
//! a stall on one output stream (its callback late while the producer keeps
//! pushing) left that route's ring fuller for good. Main and FRFR then played
//! the same note milliseconds apart in the same room — the "sound stacked on
//! the other" that only a chain off/on cured.

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

const QUANTUM: &str = "coreaudio:quantum";
const DEVICE_CHANNELS: usize = 26;
const FRAMES: usize = 128;
const GUITAR_IN: usize = 0;
const MAIN_OUT: [usize; 2] = [0, 1];
const FRFR_OUT: [usize; 2] = [24, 25];

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(QUANTUM.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "guitarra-1".into(),
        name: "Guitarra 1".into(),
        inputs: vec![endpoint("in", ChannelMode::Mono, &[GUITAR_IN])],
        outputs: vec![
            endpoint("main", ChannelMode::Stereo, &MAIN_OUT),
            endpoint("frfr", ChannelMode::Stereo, &FRFR_OUT),
        ],
    }]
}

fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-4".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into()],
        blocks: vec![AudioBlock {
            id: BlockId("amp".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    }
}

fn runtime() -> Arc<ChainRuntimeState> {
    let runtimes = build_per_input_runtimes(
        &chain(),
        44_100.0,
        &HashMap::new(),
        &vec![DEFAULT_ELASTIC_TARGET; 2],
        &registry(),
    )
    .expect("the chain must build");
    assert_eq!(runtimes.len(), 1, "one binding, one input = one runtime");
    Arc::new(runtimes.into_iter().next().unwrap().1)
}

/// Frame (counted from the pulse) at which each route first plays the pulse.
struct Rig {
    runtime: Arc<ChainRuntimeState>,
    input: Vec<f32>,
    out: Vec<f32>,
}

impl Rig {
    fn new() -> Self {
        Self {
            runtime: runtime(),
            input: vec![0.0; FRAMES * DEVICE_CHANNELS],
            out: vec![0.0; FRAMES * DEVICE_CHANNELS],
        }
    }

    /// One device period: the input callback, then the output callback of
    /// every route in `served`. A route left out is a stalled output stream.
    /// Returns, per route, the first frame of this period carrying signal.
    fn period(&mut self, pulse: bool, served: &[usize]) -> [Option<usize>; 2] {
        self.input.fill(0.0);
        if pulse {
            self.input[GUITAR_IN] = 0.5;
        }
        process_input_f32(&self.runtime, 0, &self.input, DEVICE_CHANNELS);
        let mut hit = [None, None];
        for &route in served {
            self.out.fill(0.0);
            process_output_f32(&self.runtime, route, &mut self.out, DEVICE_CHANNELS);
            let ch = [MAIN_OUT[0], FRFR_OUT[0]][route];
            hit[route] = self
                .out
                .chunks_exact(DEVICE_CHANNELS)
                .position(|frame| frame[ch].abs() > 1e-3);
        }
        hit
    }

    /// Latency of each route, in frames, for a pulse sent now.
    fn pulse_latency(&mut self) -> [usize; 2] {
        let mut latency = [None, None];
        for p in 0..64 {
            let hit = self.period(p == 0, &[0, 1]);
            for route in 0..2 {
                if latency[route].is_none() {
                    latency[route] = hit[route].map(|f| p * FRAMES + f);
                }
            }
        }
        latency.map(|l| l.expect("the pulse must come out of every route"))
    }
}

#[test]
fn a_stalled_output_route_returns_to_the_latency_of_its_sibling() {
    let mut rig = Rig::new();
    for _ in 0..200 {
        rig.period(false, &[0, 1]);
    }
    let before = rig.pulse_latency();
    assert_eq!(
        before[0], before[1],
        "precondition: both routes start with the same latency"
    );

    // The FRFR stream misses two periods; the guitar keeps producing.
    for _ in 0..2 {
        rig.period(false, &[0]);
    }
    // Then the rig sits idle for ~3 s, as the owner's did.
    for _ in 0..1_000 {
        rig.period(false, &[0, 1]);
    }

    let after = rig.pulse_latency();
    assert_eq!(
        after[1],
        after[0],
        "the FRFR route kept {} extra frames after its stream stalled — \
         Main and FRFR now play the same note apart",
        after[1] as i64 - after[0] as i64
    );
    assert_eq!(
        after, before,
        "no route may keep latency it gained in a stall"
    );
}
