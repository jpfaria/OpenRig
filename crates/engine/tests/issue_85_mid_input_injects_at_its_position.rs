//! Issue #85 — an `Input` block placed MID-chain must bring ITS OWN E/S signal
//! into the chain AT ITS POSITION, and that signal must reach the tail output
//! through the blocks that follow it.
//!
//! The oracle is a chain whose FIRST block is a gate slammed shut (so whatever
//! comes from the chain's own head input is silence at the tail) followed by a
//! mid `Input`. Feeding the mid input's stream with a tone must therefore be
//! audible at the tail: anything that arrives there came in through the port.

use std::sync::{Arc, Once};

use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{schema_for_block_model, AudioBlock, AudioBlockKind, CoreBlock, InputBlock};
use project::chain::Chain;
use project::param::ParameterSet;

const SR: f32 = 48_000.0;
const BUF: usize = 64;
const DEVICE_CHANNELS: usize = 4;
const TONE_AMP: f32 = 0.5;

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        block_dyn::register_natives();
    });
}

/// A gate that can never open: threshold at 100 % maps to 0 dBFS, so a
/// -6 dBFS tone stays below it.
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

/// The chain's own E/S (`io`) plus the second E/S the mid input reads from.
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
            inputs: vec![endpoint("aux-in", ChannelMode::Mono, vec![2])],
            outputs: Vec::new(),
        },
    ]
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

fn mid_input_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("issue85:mid-input".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "aux-in".into(),
        }),
    }
}

/// Gate at block 0, mid input at block 1.
fn chain_with_gate(gate_enabled: bool) -> Chain {
    Chain {
        id: ChainId("issue-85-mid-input".into()),
        description: Some("issue #85 mid-chain input".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![gate_block(gate_enabled), mid_input_block()],
        di_output: None,
        loopers: vec![],
    }
}

/// Feeds a 440 Hz tone into ONE device channel (silence on the others) through
/// the single cpal input stream, and returns the steady-state peak on the tail
/// output's channels. Both endpoints live on the same device, so they arrive in
/// the same callback buffer — channel 0 is the chain's head input, channel 2 is
/// the mid input's E/S.
fn drive_and_peak(runtime: &Arc<ChainRuntimeState>, tone_channel: usize) -> f32 {
    const CALLBACKS: usize = 400;
    const WARMUP: usize = 300;

    let mut input = vec![0.0_f32; BUF * DEVICE_CHANNELS];
    let mut output = vec![0.0_f32; BUF * DEVICE_CHANNELS];
    let mut peak = 0.0_f32;
    let mut phase = 0.0_f32;
    let step = 2.0 * std::f32::consts::PI * 440.0 / SR;

    for cb in 0..CALLBACKS {
        for frame in input.chunks_exact_mut(DEVICE_CHANNELS) {
            frame.iter_mut().for_each(|s| *s = 0.0);
            frame[tone_channel] = TONE_AMP * phase.sin();
            phase += step;
        }
        process_input_f32(runtime, 0, &input, DEVICE_CHANNELS);
        output.iter_mut().for_each(|s| *s = 0.0);
        process_output_f32(runtime, 0, &mut output, DEVICE_CHANNELS);
        if cb < WARMUP {
            continue;
        }
        for frame in output.chunks_exact(DEVICE_CHANNELS) {
            peak = peak.max(frame[0].abs()).max(frame[1].abs());
        }
    }
    peak
}

/// Control: with the gate bypassed the chain is a wire, so the chain's own head
/// input reaches the tail. Pins that the tail route is live, so a silent tail in
/// the test below is about the mid input, not about the route.
#[test]
fn the_head_input_reaches_the_tail_when_the_chain_is_a_wire() {
    init_registry();
    let chain = chain_with_gate(false);
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime must build with a mid-chain input block"),
    );

    let peak = drive_and_peak(&runtime, 0);
    assert!(peak > 0.1, "tail output is silent (peak {peak})");
}

#[test]
fn a_mid_input_brings_its_own_signal_into_the_chain() {
    init_registry();
    let chain = chain_with_gate(true);
    let runtime = Arc::new(
        build_chain_runtime_state(&chain, SR, &[DEFAULT_ELASTIC_TARGET], &registry())
            .expect("runtime must build with a mid-chain input block"),
    );

    // Channel 0 is the chain's own input — gated to silence. Channel 2 is the
    // mid input's own E/S, which enters AFTER the gate.
    let peak = drive_and_peak(&runtime, 2);
    assert!(
        peak > 0.1,
        "#85: the mid Input's signal never reaches the tail (peak {peak}) — a port \
         the user placed mid-chain brings its E/S in at its own position"
    );
}
