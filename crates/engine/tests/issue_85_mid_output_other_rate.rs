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
    const CALLBACKS: usize = 8_000;

    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * 2 * DEVICE_CHANNELS];
    let mut peak = 0.0_f32;
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / CHAIN_RATE as f32;
    // Fractional accumulator: how many frames the faster device owes us.
    let mut owed = 0.0_f64;

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
        }
    }
    (runtime.underrun_count(), peak)
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
