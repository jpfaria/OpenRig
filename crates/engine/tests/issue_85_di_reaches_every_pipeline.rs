//! Issue #85 — with a DI loop armed, EVERY pipeline fed by the chain's input
//! must play it, including the one that ends at a mid `Output`.
//!
//! #699 made the loop play exactly once per chain by feeding it to segment 0
//! and silence to every other segment — correct when the other segments were
//! split-mono siblings summing into the SAME route. Under the (input × output)
//! model each pipeline owns its OWN route, so silencing them makes the mid
//! output go quiet the moment the DI is on: the owner presses play and only the
//! chain's tail sounds.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::di_loop::DiLoop;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_endpoints::resolve_chain_io;
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const DEVICE_CHANNELS: usize = 4;

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

fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "main".into(),
            name: "MAIN".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("dev".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![endpoint("main-out", "dev", vec![0, 1])],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: Vec::new(),
            outputs: vec![endpoint("aux-out", "dev", vec![2, 3])],
        },
    ]
}

/// `[wire, mid Output]`: the tail pipeline runs the wire, the mid one ends at
/// the port.
fn chain() -> Chain {
    Chain {
        id: ChainId("issue-85-di-pipelines".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".into()],
        blocks: vec![
            AudioBlock {
                id: BlockId("wire".into()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "gain".into(),
                    model: "volume".into(),
                    params: ParameterSet::default(),
                }),
            },
            AudioBlock {
                id: BlockId("mid-out".into()),
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

fn route_index(chain: &Chain, registry: &[IoBinding], channels: &[usize]) -> usize {
    resolve_chain_io(chain, registry)
        .1
        .iter()
        .position(|o| o.channels == channels)
        .unwrap_or_else(|| panic!("no output route on channels {channels:?}"))
}

/// Drives SILENT device input with the DI loop armed and returns each route's
/// steady-state peak — everything heard came from the loop.
fn peaks_with_di(runtime: &Arc<ChainRuntimeState>, routes: &[(usize, [usize; 2])]) -> Vec<f32> {
    const CALLBACKS: usize = 400;
    const WARMUP: usize = 300;

    let silence = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * DEVICE_CHANNELS];
    let mut peaks = vec![0.0_f32; routes.len()];

    for cb in 0..CALLBACKS {
        process_input_f32(runtime, 0, &silence, 1);
        for (slot, (index, channels)) in routes.iter().enumerate() {
            output.iter_mut().for_each(|s| *s = 0.0);
            process_output_f32(runtime, *index, &mut output, DEVICE_CHANNELS);
            if cb < WARMUP {
                continue;
            }
            for frame in output.chunks_exact(DEVICE_CHANNELS) {
                peaks[slot] = peaks[slot]
                    .max(frame[channels[0]].abs())
                    .max(frame[channels[1]].abs());
            }
        }
    }
    peaks
}

#[test]
fn an_armed_di_loop_plays_on_the_mid_output_too() {
    init_registry();
    let chain = chain();
    let registry = registry();
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("runtime must build"),
    );
    // A steady tone as the loop, so any silence is the routing, not the source.
    let samples: Vec<f32> = (0..4_800)
        .map(|i| 0.5 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / SR).sin())
        .collect();
    runtime.set_di_loop(Some(Arc::new(DiLoop::from_samples(
        &samples, SR as u32, 1, SR as u32, 0,
    ))));

    let tail = route_index(&chain, &registry, &[0, 1]);
    let mid = route_index(&chain, &registry, &[2, 3]);
    let peaks = peaks_with_di(&runtime, &[(tail, [0, 1]), (mid, [2, 3])]);

    assert!(
        peaks[0] > 0.1,
        "the tail is silent with the DI on (peak {})",
        peaks[0]
    );
    assert!(
        peaks[1] > 0.1,
        "#85: the mid Output is silent while the DI plays (peak {}) — every \
         pipeline fed by this input must play the loop, each into its own route",
        peaks[1]
    );
}
