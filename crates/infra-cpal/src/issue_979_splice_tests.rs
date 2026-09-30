//! #979 — the "looped, stacked" sound. On the owner's rig (29/09 captures,
//! CoreAudio process tap on `rig:input-7`) the broken state plays a splice
//! every 256 frames — a discontinuity at a fixed phase, 16x the median third
//! difference — while its route rests at 256 frames per callback start with
//! frozen underruns; after a chain off/on the same guitar plays with no
//! periodic splice (1.1x) and the route rests at 128.
//!
//! So whatever disturbance a route went through, once it is over the route
//! must play the input back continuously: a sine in, the same sine out, with
//! no jump at any buffer boundary. Driven with the same simulated HAL cycle as
//! `issue_979_slack_rest_tests` (input callback, output callbacks, then the
//! #670 worker's hand-off), but the input carries a continuous sine instead of
//! a constant, so a route that drops or replays frames shows up as a splice.

use std::collections::{HashMap, VecDeque};
use std::f32::consts::TAU;
use std::sync::Arc;

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{
    build_per_input_runtime_states, process_input_f32, process_output_f32, ChainRuntimeState,
};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;

use crate::elastic::{elastic_targets, StreamClock};

const HD8: &str = "coreaudio:TUSBAudio:Fender:Quantum HD 8";
const HD8_CHANNELS: usize = 30;
const RATE: usize = 44_100;
const BUFFERS: [usize; 3] = [32, 64, 128];
const OUTPUTS: usize = 4;
const TONE_HZ: f32 = 220.0;
const AMPLITUDE: f32 = 0.1;

fn cycles(frames: usize, ms: usize) -> usize {
    RATE * ms / 1000 / frames
}

fn endpoint(name: &str, mode: ChannelMode, channels: &[usize]) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(HD8.into()),
        mode,
        channels: channels.to_vec(),
    }
}

fn guitar(id: &str, input: usize) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs: vec![endpoint("guitar", ChannelMode::Mono, &[input])],
        outputs: vec![
            endpoint("Main L/R (Out 1/2)", ChannelMode::Stereo, &[0, 1]),
            endpoint("Out 2", ChannelMode::Stereo, &[10, 11]),
        ],
    }
}

fn registry() -> Vec<IoBinding> {
    vec![guitar("guitarra-1", 0), guitar("guitarra-2", 1)]
}

/// `rig:input-7` reduced to a linear chain (a volume block), so the output is
/// the input scaled and any discontinuity is the route's.
fn chain() -> Chain {
    Chain {
        id: ChainId("rig:input-7".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["guitarra-1".into(), "guitarra-2".into()],
        blocks: vec![AudioBlock {
            id: BlockId("preamp".into()),
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

#[derive(Clone, Copy, Debug)]
enum Start {
    Lockstep,
    OutputsAfterTheInput,
    OutputsBeforeTheInput(usize),
}

#[derive(Clone, Copy, Debug)]
enum Disturbance {
    /// Every worker late by this many periods, every ~100 ms.
    LateWorker(usize),
    /// This many output cycles missed in a row, every ~100 ms.
    StalledOutputs(usize),
    /// One input buffer lost to an overload, every ~100 ms.
    LostInput,
}

struct Rig {
    frames: usize,
    runtimes: Vec<Arc<ChainRuntimeState>>,
    units: Vec<(usize, usize)>,
    starts: Vec<usize>,
    input_from: usize,
    /// Per runtime: input buffers its worker holds and has not pushed.
    backlog: Vec<VecDeque<Vec<f32>>>,
    phase: f32,
    /// Per unit: every sample it played on its first active channel.
    played: Vec<Vec<f32>>,
    now: usize,
}

impl Rig {
    fn new(frames: usize, start: Start) -> Self {
        let chain = chain();
        let registry = registry();
        let clock = StreamClock {
            device_id: HD8,
            buffer_frames: frames as u32,
        };
        let targets = elastic_targets(&chain, &registry, &[clock], &[clock; OUTPUTS]);
        let runtimes: Vec<Arc<ChainRuntimeState>> = build_per_input_runtime_states(
            &chain,
            RATE as f32,
            &HashMap::new(),
            &targets,
            &registry,
        )
        .expect("the chain must build")
        .into_iter()
        .map(|(_, runtime)| runtime)
        .collect();
        let units: Vec<(usize, usize)> = runtimes
            .iter()
            .enumerate()
            .flat_map(|(rt, runtime)| {
                runtime
                    .take_output_route_stats()
                    .into_iter()
                    .map(move |stats| (rt, stats.route))
            })
            .collect();
        let n = units.len();
        let (input_from, starts) = match start {
            Start::Lockstep => (0, vec![0; n]),
            Start::OutputsAfterTheInput => (
                0,
                units
                    .iter()
                    .map(|(_, route)| cycles(frames, 25 + 50 * route))
                    .collect(),
            ),
            Start::OutputsBeforeTheInput(ms) => (cycles(frames, ms), vec![0; n]),
        };
        Self {
            frames,
            backlog: vec![VecDeque::new(); runtimes.len()],
            runtimes,
            units,
            starts,
            input_from,
            phase: 0.0,
            played: vec![Vec::new(); n],
            now: 0,
        }
    }

    /// The next HAL input buffer: the sine on both guitar inputs.
    fn next_input(&mut self) -> Vec<f32> {
        let step = TAU * TONE_HZ / RATE as f32;
        let mut buffer = vec![0.0; self.frames * HD8_CHANNELS];
        for frame in buffer.chunks_exact_mut(HD8_CHANNELS) {
            let sample = AMPLITUDE * self.phase.sin();
            frame[0] = sample;
            frame[1] = sample;
            self.phase = (self.phase + step) % TAU;
        }
        buffer
    }

    fn cycle(&mut self, handed_over: bool, late: bool, outputs: bool) {
        if self.now >= self.input_from {
            let buffer = self.next_input();
            if handed_over {
                for pending in &mut self.backlog {
                    pending.push_back(buffer.clone());
                }
            }
        }
        if outputs {
            for i in 0..self.units.len() {
                if self.now < self.starts[i] {
                    continue;
                }
                let (rt, route) = self.units[i];
                let mut out = vec![0.0; self.frames * HD8_CHANNELS];
                process_output_f32(&self.runtimes[rt], route, &mut out, HD8_CHANNELS);
                let channel = if route % 2 == 0 { 0 } else { 10 };
                self.played[i].extend(out.chunks_exact(HD8_CHANNELS).map(|f| f[channel]));
            }
        }
        if !late {
            for rt in 0..self.runtimes.len() {
                while let Some(buffer) = self.backlog[rt].pop_front() {
                    process_input_f32(&self.runtimes[rt], 0, &buffer, HD8_CHANNELS);
                }
            }
        }
        self.now += 1;
    }

    fn run(&mut self, cycles: usize) {
        for _ in 0..cycles {
            self.cycle(true, false, true);
        }
    }

    fn disturb(&mut self, disturbance: Disturbance) {
        match disturbance {
            Disturbance::LateWorker(periods) => {
                for _ in 0..periods {
                    self.cycle(true, true, true);
                }
                self.cycle(true, false, true);
            }
            Disturbance::StalledOutputs(stalled) => {
                for _ in 0..stalled {
                    self.cycle(true, false, false);
                }
            }
            Disturbance::LostInput => self.cycle(false, false, true),
        }
    }
}

/// The largest third difference in `samples` over the one a clean sine of
/// `TONE_HZ` at `AMPLITUDE` has: 1 for a continuous tone, far above it at a
/// splice.
fn worst_splice(samples: &[f32]) -> f32 {
    let step = TAU * TONE_HZ / RATE as f32;
    let clean = AMPLITUDE * step.powi(3);
    samples
        .windows(4)
        .map(|w| (w[3] - 3.0 * w[2] + 3.0 * w[1] - w[0]).abs())
        .fold(0.0, f32::max)
        / clean
}

#[test]
fn once_the_disturbances_stop_every_route_plays_its_input_without_a_splice() {
    let mut failures = Vec::new();
    for frames in BUFFERS {
        let every = cycles(frames, 100);
        for start in [
            Start::Lockstep,
            Start::OutputsAfterTheInput,
            Start::OutputsBeforeTheInput(15),
            Start::OutputsBeforeTheInput(50),
        ] {
            for disturbance in [
                Disturbance::LateWorker(1),
                Disturbance::LateWorker(2),
                Disturbance::LateWorker(3),
                Disturbance::StalledOutputs(1),
                Disturbance::StalledOutputs(2),
                Disturbance::StalledOutputs(3),
                Disturbance::LostInput,
            ] {
                let mut rig = Rig::new(frames, start);
                rig.run(cycles(frames, 2_000));
                for _ in 0..30 {
                    rig.disturb(disturbance);
                    rig.run(every);
                }
                rig.run(cycles(frames, 1_000));
                let from: Vec<usize> = rig.played.iter().map(Vec::len).collect();
                rig.run(cycles(frames, 2_000));
                for (i, played) in rig.played.iter().enumerate() {
                    let tail = &played[from[i]..];
                    let loudest = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                    let splice = worst_splice(tail);
                    if loudest < AMPLITUDE / 2.0 || splice > 20.0 {
                        failures.push(format!(
                            "  {frames} frames, {start:?}, {disturbance:?}, unit {:?}: \
                             peak {loudest:.3}, worst splice {splice:.0}x a clean tone",
                            rig.units[i]
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "routes kept splicing after the disturbances stopped ({}):\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A seeded mix of every disturbance, back to back and overlapping, the way a
/// loaded machine (the 29/09 capture: load 9.5 on 11 cores) delivers them.
#[test]
fn after_a_random_mix_of_disturbances_every_route_plays_its_input_without_a_splice() {
    let mut failures = Vec::new();
    let mut seed: u64 = 0x979;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as usize) % n
    };
    for frames in BUFFERS {
        for start in [
            Start::Lockstep,
            Start::OutputsAfterTheInput,
            Start::OutputsBeforeTheInput(15),
            Start::OutputsBeforeTheInput(50),
        ] {
            for trial in 0..8 {
                let mut rig = Rig::new(frames, start);
                rig.run(cycles(frames, 1_000));
                for _ in 0..400 {
                    match next(6) {
                        0 => rig.disturb(Disturbance::LateWorker(1 + next(5))),
                        1 => rig.disturb(Disturbance::StalledOutputs(1 + next(5))),
                        2 => rig.disturb(Disturbance::LostInput),
                        _ => {}
                    }
                    rig.run(next(8));
                }
                rig.run(cycles(frames, 1_000));
                let from: Vec<usize> = rig.played.iter().map(Vec::len).collect();
                rig.run(cycles(frames, 2_000));
                for (i, played) in rig.played.iter().enumerate() {
                    let tail = &played[from[i]..];
                    let loudest = tail.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                    let splice = worst_splice(tail);
                    if loudest < AMPLITUDE / 2.0 || splice > 20.0 {
                        failures.push(format!(
                            "  {frames} frames, {start:?}, trial {trial}, unit {:?}: \
                             peak {loudest:.3}, worst splice {splice:.0}x a clean tone",
                            rig.units[i]
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "routes kept splicing after a random mix of disturbances ({}):\n{}",
        failures.len(),
        failures
            .iter()
            .take(12)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
