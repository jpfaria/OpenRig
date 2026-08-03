//! Two-clock harness for the issue #85 cross-rate oracle (split from
//! `issue_85_mid_output_other_rate.rs`).
//!
//! Everything here is fixture and measurement — the rig (a chain clocked by the
//! Scarlett at 44.1 kHz with a mid `Output` on a TEYUN that cannot go below
//! 48 kHz), the driver that runs the two clocks against each other, and the
//! residual measurement that says whether what the device popped still sounds
//! like the chain's own tail. No assertion lives here: the oracle itself stays
//! in the test file so #85's contract reads in one place.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_endpoints::resolve_chain_io;
use engine::runtime_state::ChainRuntimeState;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

/// The chain's clock (the Scarlett).
pub const CHAIN_RATE: u32 = 44_100;
/// The tap's device clock (the TEYUN, whose floor is 48 kHz).
pub const TAP_RATE: u32 = 48_000;
pub const BUF: usize = 64;
pub const DEVICE_CHANNELS: usize = 4;
pub const TONE_AMP: f32 = 0.5;

pub fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_dyn::register_natives();
    });
}

fn endpoint(name: &str, device: &str, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(device.into()),
        mode: ChannelMode::Stereo,
        channels,
    }
}

/// The chain's own E/S at 44.1 kHz plus the tap's E/S on another device.
pub fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("scarlett".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![endpoint("main-out", "scarlett", vec![0, 1])],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: vec![IoEndpoint {
                name: "aux-in".into(),
                device_id: DeviceId("teyun".into()),
                mode: ChannelMode::Mono,
                channels: vec![1],
            }],
            outputs: vec![endpoint("aux-out", "teyun", vec![2, 3])],
        },
    ]
}

/// The owner's shape: a mid `Input` from the other interface AND a mid `Output`
/// to it, both inside the chain.
pub fn chain_with_mid_input() -> Chain {
    let mut chain = chain();
    chain.blocks.insert(
        1,
        AudioBlock {
            id: BlockId("issue85:mid-input".into()),
            enabled: true,
            kind: AudioBlockKind::Input(project::block::InputBlock {
                model: "standard".into(),
                io: "aux".into(),
                endpoint: "aux-in".into(),
            }),
        },
    );
    chain
}

pub fn chain() -> Chain {
    Chain {
        id: ChainId("issue-85-other-rate".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![
            AudioBlock {
                id: BlockId("issue85:wire".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            },
            AudioBlock {
                id: BlockId("issue85:mid-output".into()),
                enabled: true,
                kind: AudioBlockKind::Output(OutputBlock {
                    model: "standard".into(),
                    io: "aux".into(),
                    endpoint: "aux-out".into(),
                }),
            },
        ],
        di_output: None,
        loopers: vec![],
    }
}

/// Route index of the endpoint writing `channels`.
pub fn route_index(chain: &Chain, registry: &[IoBinding], channels: &[usize]) -> usize {
    resolve_chain_io(chain, registry)
        .1
        .iter()
        .position(|o| o.channels == channels)
        .unwrap_or_else(|| panic!("no output route on channels {channels:?}"))
}

/// Runs the two clocks against each other: the chain's input callback delivers
/// `BUF` frames at 44.1 kHz while the tap's device pops the frames its own
/// callback would ask for in the same wall-clock time.
///
/// `device_rate` is what the tap's device ACTUALLY runs at. Two interfaces have
/// two crystals: a TEYUN nominally at 48 kHz runs a hair fast or slow against a
/// Scarlett's 44.1 kHz, and a fixed conversion ratio accumulates that error
/// until the route runs dry — 256 underruns per poll on the owner's rig, with
/// zero xruns.
pub fn run_both_clocks(
    runtime: &Arc<ChainRuntimeState>,
    tap_route: usize,
    device_rate: f64,
) -> (u64, f32) {
    let (u, p, _) = run_both_clocks_capturing(runtime, tap_route, device_rate);
    (u, p)
}

/// Same run, but keeping every frame the device popped — the only way to hear
/// what actually left the interface. A counter cannot tell "clean" from
/// "horrível": a converter that never starves can still alias, click at every
/// wrap, or drift in pitch.
fn run_both_clocks_capturing(
    runtime: &Arc<ChainRuntimeState>,
    tap_route: usize,
    device_rate: f64,
) -> (u64, f32, Vec<f32>) {
    let (u, p, tap, _) = run_capturing_both(runtime, tap_route, device_rate);
    (u, p, tap)
}

/// Captures the TAP (on the device's clock) and the chain's own TAIL (on the
/// chain's clock) in the same run. The tail is the reference: it carries the
/// same processing without any conversion, so comparing the two isolates what
/// the conversion did — a fixed probe that cannot blame the resampler for the
/// chain's own tone.
pub fn run_capturing_both(
    runtime: &Arc<ChainRuntimeState>,
    tap_route: usize,
    device_rate: f64,
) -> (u64, f32, Vec<f32>, Vec<f32>) {
    const CALLBACKS: usize = 8_000;

    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * 2 * DEVICE_CHANNELS];
    let mut peak = 0.0_f32;
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / CHAIN_RATE as f32;
    // Fractional accumulator: how many frames the faster device owes us.
    let mut owed = 0.0_f64;
    let mut captured: Vec<f32> = Vec::new();
    let mut tail_captured: Vec<f32> = Vec::new();

    for _ in 0..CALLBACKS {
        for s in input.iter_mut() {
            *s = TONE_AMP * phase.sin();
            phase += step;
        }
        process_input_f32(runtime, 0, &input, 1);

        owed += BUF as f64 * device_rate / CHAIN_RATE as f64;
        let frames = owed.floor() as usize;
        owed -= frames as f64;
        let len = frames * DEVICE_CHANNELS;
        output[..len].iter_mut().for_each(|s| *s = 0.0);
        process_output_f32(runtime, tap_route, &mut output[..len], DEVICE_CHANNELS);
        for frame in output[..len].chunks_exact(DEVICE_CHANNELS) {
            peak = peak.max(frame[2].abs()).max(frame[3].abs());
            captured.push(frame[2]);
        }
        // The chain's own tail, on the chain's clock: BUF frames per callback.
        let tail_len = BUF * DEVICE_CHANNELS;
        output[..tail_len].iter_mut().for_each(|s| *s = 0.0);
        process_output_f32(runtime, 0, &mut output[..tail_len], DEVICE_CHANNELS);
        for frame in output[..tail_len].chunks_exact(DEVICE_CHANNELS) {
            tail_captured.push(frame[0]);
        }
    }
    (runtime.underrun_count(), peak, captured, tail_captured)
}

/// Worst per-window residual: how much of the captured signal is NOT the tone,
/// in dB relative to it, measured over short windows and reported as the
/// median-of-worst. Short windows on purpose — a slow pitch trim (the clock
/// tracking bends the ratio by at most 0.2 %) must NOT count as distortion,
/// while aliasing, a click at a buffer wrap or a dropout must.
pub fn worst_window_residual_db(samples: &[f32], rate: f64) -> f32 {
    const WINDOW: usize = 1024;
    let mut per_window: Vec<f32> = samples
        .chunks_exact(WINDOW)
        .map(|w| residual_db(w, rate))
        .collect();
    if per_window.is_empty() {
        return residual_db(samples, rate);
    }
    per_window.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // The 90th percentile: one bad window is noise, a tenth of them is the sound.
    per_window[per_window.len() * 9 / 10]
}

/// Residual of one window against the best-fitting 440 Hz sine.
fn residual_db(samples: &[f32], rate: f64) -> f32 {
    let w = 2.0 * std::f64::consts::PI * 440.0 / rate;
    // Best-fit amplitude/phase of a 440 Hz sine (single-bin DFT).
    let (mut re, mut im) = (0.0_f64, 0.0_f64);
    for (i, &s) in samples.iter().enumerate() {
        let t = i as f64 * w;
        re += s as f64 * t.cos();
        im += s as f64 * t.sin();
    }
    let n = samples.len() as f64;
    let (a, b) = (2.0 * re / n, 2.0 * im / n);
    let mut tone_sq = 0.0_f64;
    let mut err_sq = 0.0_f64;
    for (i, &s) in samples.iter().enumerate() {
        let t = i as f64 * w;
        let fit = a * t.cos() + b * t.sin();
        tone_sq += fit * fit;
        err_sq += (s as f64 - fit) * (s as f64 - fit);
    }
    (10.0 * (err_sq.max(1e-30) / tone_sq.max(1e-30)).log10()) as f32
}

/// Drives the tap the way a real device does — fixed buffers at its own rate,
/// every sixteenth callback bunched into two, the way the OS hands over two
/// periods at once — and returns how many times the route ran dry.
pub fn starves_under_jitter(runtime: &Arc<ChainRuntimeState>, tap: usize) -> u64 {
    const CALLBACKS: usize = 4_000;
    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * 4 * DEVICE_CHANNELS];
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / CHAIN_RATE as f32;
    let mut due = 0.0_f64;
    let before = runtime.underrun_count();
    for cb in 0..CALLBACKS {
        for s in input.iter_mut() {
            *s = TONE_AMP * phase.sin();
            phase += step;
        }
        process_input_f32(runtime, 0, &input, 1);
        due += BUF as f64 * TAP_RATE as f64 / CHAIN_RATE as f64;
        let bunched = cb % 16 == 0;
        while due >= BUF as f64 {
            let frames = if bunched { BUF * 2 } else { BUF };
            let len = frames * DEVICE_CHANNELS;
            output[..len].iter_mut().for_each(|s| *s = 0.0);
            process_output_f32(runtime, tap, &mut output[..len], DEVICE_CHANNELS);
            due -= frames as f64;
            if bunched {
                break;
            }
        }
    }
    runtime.underrun_count() - before
}
