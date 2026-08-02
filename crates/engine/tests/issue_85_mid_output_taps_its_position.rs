//! Issue #85 — an `Output` block placed MID-chain must emit the signal AS
//! PROCESSED UP TO ITS POSITION, not the end-of-chain signal. The chain keeps
//! flowing through the blocks that follow it down to the final output; the mid
//! output is a non-destructive tap at its own point.
//!
//! The oracle is a chain whose FIRST block is a mid `Output` (so its tap point
//! is the raw input) followed by a noise gate slammed shut (threshold at 0 dBFS,
//! no hold, no hysteresis). The tail output is therefore silence, while the mid
//! output — sitting BEFORE the gate — must still carry the tone.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_endpoints::resolve_chain_io;
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const DEVICE_CHANNELS: usize = 6;
const TONE_AMP: f32 = 0.5;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_dyn::register_natives();
    });
}

/// A gate that can never open: threshold at 100 % maps to 0 dBFS, so a
/// -6 dBFS tone stays below it. Hold and hysteresis are zeroed so the gate
/// settles closed within a couple of milliseconds.
fn slammed_shut_gate() -> ParameterSet {
    let schema =
        schema_for_block_model("dynamics", "gate_basic").expect("gate_basic schema must exist");
    let mut ps = ParameterSet::default();
    ps.insert("threshold", ParameterValue::Float(100.0));
    ps.insert("attack_ms", ParameterValue::Float(0.1));
    ps.insert("release_ms", ParameterValue::Float(1.0));
    ps.insert("hold_ms", ParameterValue::Float(0.0));
    ps.insert("hysteresis_db", ParameterValue::Float(0.0));
    ps.normalized_against(&schema)
        .expect("gate params must normalize")
}

fn endpoint(name: &str, mode: ChannelMode, channels: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId("dev".into()),
        mode,
        channels,
    }
}

/// The chain's own E/S (`main`, channels 0-1) plus a second E/S the mid output
/// block writes to (`aux`, channels 2-3 of the same interface).
fn registry() -> Vec<IoBinding> {
    vec![
        IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![endpoint("in0", ChannelMode::Mono, vec![0])],
            outputs: vec![endpoint("main", ChannelMode::Stereo, vec![0, 1])],
        },
        IoBinding {
            id: "aux".into(),
            name: "AUX".into(),
            inputs: Vec::new(),
            outputs: vec![endpoint("aux", ChannelMode::Stereo, vec![2, 3])],
        },
        IoBinding {
            id: "aux2".into(),
            name: "AUX 2".into(),
            inputs: Vec::new(),
            outputs: vec![endpoint("aux2", ChannelMode::Stereo, vec![4, 5])],
        },
    ]
}

fn output_block(id: &str, io: &str, endpoint: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: io.into(),
            endpoint: endpoint.into(),
        }),
    }
}

fn gate_block(enabled: bool) -> AudioBlock {
    AudioBlock {
        id: BlockId("issue85:gate".into()),
        enabled,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "dynamics".into(),
            model: "gate_basic".into(),
            params: slammed_shut_gate(),
        }),
    }
}

/// Mid output at block 0, gate at block 1.
fn chain_with_gate(gate_enabled: bool) -> Chain {
    Chain {
        id: ChainId("issue-85-mid-output".into()),
        description: Some("issue #85 mid-chain output tap".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            output_block("issue85:mid-output", "aux", "aux"),
            gate_block(gate_enabled),
        ],
        di_output: None,
        loopers: vec![],
    }
}

/// `[Output(aux) → gate → Output(aux2)]`: one tap on each side of the gate.
fn chain_tapping_both_sides_of_the_gate() -> Chain {
    let mut chain = chain_with_gate(true);
    chain
        .blocks
        .push(output_block("issue85:mid-output-2", "aux2", "aux2"));
    chain
}

/// Flat index of the output route writing to `channels`, in the same order the
/// engine numbers routes with.
fn route_index(chain: &Chain, registry: &[IoBinding], channels: &[usize]) -> usize {
    resolve_chain_io(chain, registry)
        .1
        .iter()
        .position(|o| o.channels == channels)
        .unwrap_or_else(|| panic!("no output route on channels {channels:?}"))
}

/// Drives a 440 Hz tone through the runtime and returns the steady-state peak
/// of each requested route, measured on that route's own device channels.
fn drive_and_peak(runtime: &Arc<ChainRuntimeState>, routes: &[(usize, [usize; 2])]) -> Vec<f32> {
    const CALLBACKS: usize = 400;
    const WARMUP: usize = 300;

    let mut input = vec![0.0_f32; BUF];
    let mut output = vec![0.0_f32; BUF * DEVICE_CHANNELS];
    let mut peaks = vec![0.0_f32; routes.len()];
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / SR;

    for cb in 0..CALLBACKS {
        for s in input.iter_mut() {
            *s = TONE_AMP * phase.sin();
            phase += step;
        }
        process_input_f32(runtime, 0, &input, 1);
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

/// Control: with the gate bypassed the chain is a straight wire, so BOTH the
/// tail and the mid output route must stream the tone. This pins that the mid
/// output IS a live route (device, channels, drain) — so a silent mid output in
/// the test below is the routing defect, not a missing route.
#[test]
fn mid_chain_output_route_streams_audio_when_the_chain_is_a_wire() {
    init_registry();
    let chain = chain_with_gate(false);
    let registry = registry();
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("runtime must build with a mid-chain output block"),
    );

    let tail = route_index(&chain, &registry, &[0, 1]);
    let mid = route_index(&chain, &registry, &[2, 3]);
    let peaks = drive_and_peak(&runtime, &[(tail, [0, 1]), (mid, [2, 3])]);

    assert!(peaks[0] > 0.1, "tail output is silent (peak {})", peaks[0]);
    assert!(
        peaks[1] > 0.1,
        "the mid-chain output route never streams (peak {}) — its endpoint is not \
         wired as an output route at all",
        peaks[1]
    );
}

#[test]
fn mid_chain_output_emits_the_signal_at_its_own_position() {
    init_registry();
    let chain = chain_with_gate(true);
    let registry = registry();
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("runtime must build with a mid-chain output block"),
    );

    let tail = route_index(&chain, &registry, &[0, 1]);
    let mid = route_index(&chain, &registry, &[2, 3]);
    let peaks = drive_and_peak(&runtime, &[(tail, [0, 1]), (mid, [2, 3])]);
    let (tail_peak, mid_peak) = (peaks[0], peaks[1]);

    assert!(
        tail_peak < 0.02,
        "setup check: the gate after the mid output must silence the TAIL output \
         (tail peak {tail_peak})"
    );
    assert!(
        mid_peak > 0.1,
        "#85: the mid-chain output emitted the END-OF-CHAIN signal (peak {mid_peak}) \
         instead of the signal at its own position — it sits BEFORE the gate, so it \
         must still carry the tone (tail peak {tail_peak})"
    );
}

/// Two mid outputs straddling the gate: the one BEFORE it carries the tone, the
/// one AFTER it is gated like the tail. Pins that each tap emits the signal as
/// processed up to ITS position — not the raw input, not the end of the chain.
#[test]
fn each_mid_output_emits_the_signal_processed_up_to_it() {
    init_registry();
    let chain = chain_tapping_both_sides_of_the_gate();
    let registry = registry();
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("runtime must build with two mid-chain output blocks"),
    );

    let before_gate = route_index(&chain, &registry, &[2, 3]);
    let after_gate = route_index(&chain, &registry, &[4, 5]);
    let peaks = drive_and_peak(&runtime, &[(before_gate, [2, 3]), (after_gate, [4, 5])]);

    assert!(
        peaks[0] > 0.1,
        "the output BEFORE the gate must carry the tone (peak {})",
        peaks[0]
    );
    assert!(
        peaks[1] < 0.02,
        "the output AFTER the gate must be gated like the tail — it emitted the \
         pre-gate signal instead (peak {})",
        peaks[1]
    );
}
