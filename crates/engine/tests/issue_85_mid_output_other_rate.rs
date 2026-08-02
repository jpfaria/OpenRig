//! Issue #85 — a mid `Output` whose device runs at ANOTHER sample rate.
//!
//! The user's rig: the chain is clocked by the Scarlett at 44.1 kHz and the mid
//! `Output` writes to a TEYUN that cannot go below 48 kHz. The tap pushes frames
//! at the chain's rate while that device's callback pops at its own — the route
//! starves, and what comes out is the crackle he heard ("saiu o som, mas bem
//! ruim", 1024 underruns per poll with zero xruns: an elastic-buffer underrun,
//! not a CPU overrun).
//!
//! The oracle: over a run of the two clocks against each other, the mid route
//! must not underrun. Nothing about the chain's own tail changes.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_endpoints::resolve_chain_io;
use engine::runtime_graph::build_chain_runtime_state_with_device_rates;
use engine::runtime_state::ChainRuntimeState;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

/// The chain's clock (the Scarlett).
const CHAIN_RATE: u32 = 44_100;
/// The tap's device clock (the TEYUN, whose floor is 48 kHz).
const TAP_RATE: u32 = 48_000;
const BUF: usize = 64;
const DEVICE_CHANNELS: usize = 4;
const TONE_AMP: f32 = 0.5;

fn init_registry() {
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
fn registry() -> Vec<IoBinding> {
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
fn chain_with_mid_input() -> Chain {
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

fn chain() -> Chain {
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
fn route_index(chain: &Chain, registry: &[IoBinding], channels: &[usize]) -> usize {
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
fn run_both_clocks(
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
fn run_capturing_both(
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
fn worst_window_residual_db(samples: &[f32], rate: f64) -> f32 {
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

#[test]
fn a_mid_output_on_another_clock_does_not_starve() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let tap = route_index(&chain, &registry, &[2, 3]);
    // The rates the caller resolves per device: the chain is clocked by the
    // Scarlett at 44.1 kHz, the tap's TEYUN runs at 48 kHz.
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build with a mid output on another clock"),
    );

    let (underruns, peak) = run_both_clocks(&runtime, tap, TAP_RATE as f64);

    assert!(peak > 0.1, "the tap is silent (peak {peak})");
    assert_eq!(
        underruns, 0,
        "#85: the mid Output's route starved {underruns} times — the tap writes at \
         the chain's {CHAIN_RATE} Hz while its device pops at {TAP_RATE} Hz, and \
         the difference is the crackle"
    );
}

/// The owner's rig, with the detail that matters: the tap's interface does not
/// run at EXACTLY its nominal rate. 0.05 % fast is an ordinary crystal
/// tolerance, and a fixed ratio bleeds the route dry at that pace — his
/// "falhando o som na saída para TEYUN", 256 underruns per poll with 0 xruns.
/// The converter has to follow the device instead of trusting the label.
#[test]
fn a_mid_output_follows_its_devices_real_clock() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let tap = route_index(&chain, &registry, &[2, 3]);
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    // Nominally 48 kHz, actually 0.05 % fast.
    let (underruns, peak) = run_both_clocks(&runtime, tap, TAP_RATE as f64 * 1.0005);

    assert!(peak > 0.1, "the tap is silent (peak {peak})");
    assert_eq!(
        underruns, 0,
        "#85: the tap starved {underruns} times against a device drifting 0.05 % — \
         the conversion ratio must follow the route's fill, not the nominal rate"
    );
}

/// The owner's actual trigger: "se eu ligar sem alterar a ordem não dá
/// problema; agora se eu alterar a ordem começa". Moving a block rebuilds the
/// runtime in place, and the rebuild re-derives each route — if the tap's route
/// loses the device rate it was built with, the conversion stops and the route
/// starves exactly as before.
#[test]
fn reordering_the_chain_keeps_the_tap_on_its_own_clock() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    // Move the mid Output ahead of the wire block — the reorder the user does
    // by dragging a row — and rebuild in place, the way a live edit does.
    let mut reordered = chain.clone();
    let port = reordered.blocks.remove(1);
    reordered.blocks.insert(0, port);
    engine::runtime_graph::update_chain_runtime_state(
        &runtime,
        &reordered,
        CHAIN_RATE as f32,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("live rebuild after a reorder");

    let tap = route_index(&reordered, &registry, &[2, 3]);
    let (underruns, peak) = run_both_clocks(&runtime, tap, TAP_RATE as f64);

    assert!(
        peak > 0.1,
        "the tap went silent after the reorder (peak {peak})"
    );
    assert_eq!(
        underruns, 0,
        "#85: after moving a block the tap starved {underruns} times — the rebuilt \
         route must keep the device rate it converts to"
    );
}

/// His exact trigger, with his chain shape: a mid `Input` AND a mid `Output`,
/// and then he MOVES the input. Reordering rebuilds the runtime in place and
/// re-groups the per-input segments; the tap must come back converting to its
/// own device rate.
#[test]
fn moving_the_mid_input_keeps_the_tap_on_its_own_clock() {
    init_registry();
    let chain = chain_with_mid_input();
    let registry = registry();
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    // Drag the mid Input one row down.
    let mut reordered = chain.clone();
    let port = reordered.blocks.remove(1);
    reordered.blocks.insert(2, port);
    engine::runtime_graph::update_chain_runtime_state(
        &runtime,
        &reordered,
        CHAIN_RATE as f32,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("live rebuild after moving the input");

    let tap = route_index(&reordered, &registry, &[2, 3]);
    let (underruns, peak) = run_both_clocks(&runtime, tap, TAP_RATE as f64);

    assert!(
        peak > 0.1,
        "the tap went silent after moving the input (peak {peak})"
    );
    assert_eq!(
        underruns, 0,
        "#85: after moving the mid Input the tap starved {underruns} times — \
         'se eu ligar sem alterar a ordem não dá problema; se eu alterar a ordem começa'"
    );
}

/// The owner's verdict on the shipped conversion: "o som que está saindo na
/// TEYUN está horrível, só fica bom se eu colocar na mesma frequência". Zero
/// underruns proved nothing about that — this listens to what the device
/// actually popped and compares it with the chain's OWN tail, which carries the
/// same processing with no conversion at all.
#[test]
fn the_resampled_tap_sounds_like_the_chains_own_output() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let tap = route_index(&chain, &registry, &[2, 3]);
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    let (_, peak, tap_pcm, tail_pcm) = run_capturing_both(&runtime, tap, TAP_RATE as f64);
    assert!(peak > 0.1, "the tap is silent (peak {peak})");

    let tap_body = &tap_pcm[tap_pcm.len() / 2..];
    let tail_body = &tail_pcm[tail_pcm.len() / 2..];
    let tap_residual = worst_window_residual_db(tap_body, TAP_RATE as f64);
    let tail_residual = worst_window_residual_db(tail_body, CHAIN_RATE as f64);
    eprintln!(
        "[#85] residual — tap (converted) {tap_residual:.1} dB, chain tail {tail_residual:.1} dB"
    );
    assert!(
        tap_residual <= tail_residual + 6.0,
        "#85: the converted tap is {:.1} dB dirtier than the chain's own output \
         ({tap_residual:.1} vs {tail_residual:.1}) — that gap is the 'som horrível' \
         on the TEYUN",
        tap_residual - tail_residual
    );
}

/// Control: the same rig with both devices on ONE rate — no conversion at all.
/// The tap must match the chain's own tail here too, which is what makes the
/// comparison above meaningful (and is exactly what the owner hears: "só fica
/// bom se eu colocar na mesma frequência").
#[test]
fn the_same_rate_tap_matches_the_chains_output() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let tap = route_index(&chain, &registry, &[2, 3]);
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), CHAIN_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    let (_, peak, tap_pcm, tail_pcm) = run_capturing_both(&runtime, tap, CHAIN_RATE as f64);
    assert!(peak > 0.1, "the tap is silent (peak {peak})");
    let tap_residual = worst_window_residual_db(&tap_pcm[tap_pcm.len() / 2..], CHAIN_RATE as f64);
    let tail_residual =
        worst_window_residual_db(&tail_pcm[tail_pcm.len() / 2..], CHAIN_RATE as f64);
    eprintln!("[#85] same rate — tap {tap_residual:.1} dB, chain tail {tail_residual:.1} dB");
    assert!(
        tap_residual <= tail_residual + 6.0,
        "even at one rate the tap is dirtier than the chain's own output \
         ({tap_residual:.1} vs {tail_residual:.1})"
    );
}

/// Drives the tap the way a real device does — fixed buffers at its own rate,
/// every sixteenth callback bunched into two, the way the OS hands over two
/// periods at once — and returns how many times the route ran dry.
fn starves_under_jitter(runtime: &Arc<ChainRuntimeState>, tap: usize) -> u64 {
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

/// The device does not pop "exactly what it is owed" — it takes its OWN buffer
/// (64 frames) at its own callback times, and the OS lets those times wobble.
/// The owner hears the result of that wobble on the TEYUN while the Scarlett
/// (same chain, no conversion) is perfect: a cross-rate route needs a cushion
/// deep enough to ride the jitter, or it hits empty and holds the last frame —
/// which is what "horrível" sounds like.
#[test]
fn a_cross_rate_tap_rides_callback_jitter() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let tap = route_index(&chain, &registry, &[2, 3]);
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    const CALLBACKS: usize = 4_000;
    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * 4 * DEVICE_CHANNELS];
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / CHAIN_RATE as f32;
    // The device's callbacks accumulate at its own rate; each one takes exactly
    // its buffer. `wobble` delays every 16th callback and doubles the next one,
    // the way the OS bunches two periods together.
    let mut due = 0.0_f64;
    for cb in 0..CALLBACKS {
        for s in input.iter_mut() {
            *s = TONE_AMP * phase.sin();
            phase += step;
        }
        process_input_f32(&runtime, 0, &input, 1);

        due += BUF as f64 * TAP_RATE as f64 / CHAIN_RATE as f64;
        let bunched = cb % 16 == 0;
        while due >= BUF as f64 {
            let frames = if bunched { BUF * 2 } else { BUF };
            let len = frames * DEVICE_CHANNELS;
            output[..len].iter_mut().for_each(|s| *s = 0.0);
            process_output_f32(&runtime, tap, &mut output[..len], DEVICE_CHANNELS);
            due -= frames as f64;
            if bunched {
                break;
            }
        }
    }

    let underruns = runtime.underrun_count();
    assert_eq!(
        underruns, 0,
        "#85: the cross-rate tap starved {underruns} times against ordinary callback \
         jitter — its cushion has to be deep enough for two clocks"
    );
}

/// His trigger, at the layer that changes: "mudei de ordem, deu merda". Moving a
/// block rebuilds the runtime in place, and the rebuilt route must keep BOTH
/// the device rate it converts to and the deeper cushion a cross-rate route
/// needs — a fresh route sized for the lockstep case starves on the first
/// bunched callback.
#[test]
fn a_reorder_keeps_the_cross_rate_cushion() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let device_rates = std::collections::HashMap::from([
        (DeviceId("scarlett".into()), CHAIN_RATE as f32),
        (DeviceId("teyun".into()), TAP_RATE as f32),
    ]);
    let runtime = Arc::new(
        build_chain_runtime_state_with_device_rates(
            &chain,
            CHAIN_RATE as f32,
            &device_rates,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("runtime must build"),
    );

    // Drag the port one row — the live edit he does.
    let mut reordered = chain.clone();
    let port = reordered.blocks.remove(1);
    reordered.blocks.insert(0, port);
    engine::runtime_graph::update_chain_runtime_state(
        &runtime,
        &reordered,
        CHAIN_RATE as f32,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("live rebuild after the reorder");

    let tap = route_index(&reordered, &registry, &[2, 3]);
    let starved = starves_under_jitter(&runtime, tap);
    assert_eq!(
        starved, 0,
        "#85: after a reorder the tap starved {starved} times — the rebuilt route \
         lost the cross-rate cushion"
    );
}
