//! #967 — an insert switched off LIVE bypasses its loop in the DSP.
//!
//! The streams of a bound insert stay open whether it is on or off; the
//! footswitch only decides what the return segment listens to (the gear, or
//! the dry signal the send segment produced) and whether the gear is fed. These
//! drive real buffers through the built runtime and check what the owner
//! hears: the right signal on each side of the switch, no click at the switch,
//! a silent send while the loop is off, the sum of every head on the dry path,
//! and a bounded delay when the loop's interface runs on its own clock.

use std::sync::Arc;

use super::tests::{insert_chain, insert_registry};
use super::{build_chain_runtime_state, process_input_f32, process_output_f32};
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_state::ChainRuntimeState;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::{AudioBlock, AudioBlockKind, InsertBlock};
use project::chain::Chain;

const FRAMES: usize = 128;
const CHANNELS: usize = 2;
const GUITAR: f32 = 0.5;
const GEAR: f32 = 0.2;
const TAIL: usize = 0;
const SEND: usize = 1;
const INSERT: &str = "insert:0";

/// `insert_chain` without its two effect blocks: input → insert → output, so
/// the tail carries exactly the dry guitar or exactly the gear's answer.
fn loop_chain(insert_enabled: bool) -> Chain {
    let mut chain = insert_chain();
    chain
        .blocks
        .retain(|b| !matches!(b.kind, AudioBlockKind::Core(_)));
    let insert = chain
        .blocks
        .iter_mut()
        .find(|b| b.id.0 == INSERT)
        .expect("the fixture has an insert");
    insert.enabled = insert_enabled;
    chain
}

fn build(chain: &Chain, registry: &[IoBinding]) -> Arc<ChainRuntimeState> {
    Arc::new(
        build_chain_runtime_state(chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], registry)
            .expect("the chain must build a runtime"),
    )
}

/// One period of the rig: the guitar's stream (input 0), then the loop's
/// return stream (input 1, its own interface in `insert_registry`), then the
/// tail and the send drained. Returns `(tail, send)` as left-channel samples.
fn period(runtime: &Arc<ChainRuntimeState>, gear: f32) -> (Vec<f32>, Vec<f32>) {
    period_sized(runtime, gear, FRAMES, FRAMES)
}

fn period_sized(
    runtime: &Arc<ChainRuntimeState>,
    gear: f32,
    guitar_frames: usize,
    return_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    process_input_f32(
        runtime,
        0,
        &vec![GUITAR; guitar_frames * CHANNELS],
        CHANNELS,
    );
    process_input_f32(runtime, 1, &vec![gear; return_frames * CHANNELS], CHANNELS);
    let mut tail = vec![0.0_f32; FRAMES * CHANNELS];
    let mut send = vec![0.0_f32; FRAMES * CHANNELS];
    process_output_f32(runtime, TAIL, &mut tail, CHANNELS);
    process_output_f32(runtime, SEND, &mut send, CHANNELS);
    (
        tail.chunks(CHANNELS).map(|f| f[0]).collect(),
        send.chunks(CHANNELS).map(|f| f[0]).collect(),
    )
}

fn settle(runtime: &Arc<ChainRuntimeState>, gear: f32) -> (Vec<f32>, Vec<f32>) {
    let mut last = (Vec::new(), Vec::new());
    for _ in 0..256 {
        last = period(runtime, gear);
    }
    last
}

fn level(samples: &[f32]) -> f32 {
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len().max(1) as f32
}

fn toggle(runtime: &ChainRuntimeState, enabled: bool) {
    crate::runtime::set_block_enabled(runtime, &BlockId(INSERT.into()), enabled)
        .expect("the toggle must queue");
}

fn no_errors(runtime: &ChainRuntimeState) -> Vec<String> {
    let mut errors = Vec::new();
    while let Some(e) = runtime.error_queue.pop() {
        errors.push(e.message);
    }
    errors
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.02
}

#[test]
fn switching_the_insert_off_live_plays_the_dry_guitar() {
    let runtime = build(&loop_chain(true), &insert_registry());
    let (tail, _) = settle(&runtime, GEAR);
    assert!(
        close(level(&tail), GEAR),
        "loop on: the tail is the gear, got {}",
        level(&tail)
    );

    toggle(&runtime, false);
    let (tail, _) = settle(&runtime, GEAR);

    assert!(
        close(level(&tail), GUITAR),
        "loop off: the tail is the dry guitar, not the gear — got {}",
        level(&tail)
    );
    assert_eq!(
        no_errors(&runtime),
        Vec::<String>::new(),
        "the toggle must apply silently"
    );
}

#[test]
fn switching_the_insert_back_on_live_plays_the_gear() {
    let runtime = build(&loop_chain(false), &insert_registry());
    let (tail, _) = settle(&runtime, GEAR);
    assert!(
        close(level(&tail), GUITAR),
        "loop off: dry guitar, got {}",
        level(&tail)
    );

    toggle(&runtime, true);
    let (tail, _) = settle(&runtime, GEAR);

    assert!(
        close(level(&tail), GEAR),
        "loop on again: the gear's answer, got {}",
        level(&tail)
    );
    assert_eq!(no_errors(&runtime), Vec::<String>::new());
}

/// A switched-off loop is out of the signal path: the gear gets nothing, as it
/// did before #967 when a disabled insert had no send stream at all. A re-amp
/// box feeding a real amp in the room must go quiet with the footswitch.
#[test]
fn a_switched_off_insert_sends_silence_to_the_gear() {
    let runtime = build(&loop_chain(true), &insert_registry());
    let (_, send) = settle(&runtime, GEAR);
    assert!(
        close(level(&send), GUITAR),
        "loop on: the gear is fed, got {}",
        level(&send)
    );

    toggle(&runtime, false);
    let (_, send) = settle(&runtime, GEAR);

    assert!(
        level(&send) < 1e-4,
        "loop off: the send must carry silence, got {}",
        level(&send)
    );
}

/// The switch is a crossfade, never a step: no sample-to-sample jump larger
/// than a raised-cosine ramp of the level difference produces.
#[test]
fn switching_the_insert_either_way_does_not_click() {
    let runtime = build(&loop_chain(true), &insert_registry());
    let (tail, _) = settle(&runtime, GEAR);
    let mut previous = *tail.last().expect("a settled tail");
    let mut worst = 0.0_f32;
    for enabled in [false, true, false] {
        toggle(&runtime, enabled);
        for _ in 0..64 {
            let (tail, _) = period(&runtime, GEAR);
            for s in tail {
                worst = worst.max((s - previous).abs());
                previous = s;
            }
        }
    }
    assert!(
        worst < 0.02,
        "a toggle must crossfade between the gear and the dry path — largest \
         sample step was {worst}"
    );
}

/// Two heads feed one insert: the send is their SUM, so the dry path must
/// carry the same sum — not one head's buffer after the other.
#[test]
fn every_head_feeding_the_insert_is_summed_on_the_dry_path() {
    const DEV: &str = "dev";
    const DEV_CHANNELS: usize = 8;
    let mono = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEV.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let stereo = |name: &str, a: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(DEV.into()),
        mode: ChannelMode::Stereo,
        channels: vec![a, a + 1],
    };
    let registry = vec![
        IoBinding {
            id: "a".into(),
            name: "A".into(),
            inputs: vec![mono("in", 0)],
            outputs: vec![stereo("out", 0)],
        },
        IoBinding {
            id: "b".into(),
            name: "B".into(),
            inputs: vec![mono("in", 1)],
            outputs: vec![stereo("out", 2)],
        },
        IoBinding {
            id: "fx".into(),
            name: "FX".into(),
            inputs: vec![mono("ret", 4)],
            outputs: vec![mono("snd", 4)],
        },
    ];
    let chain = Chain {
        id: ChainId("two-heads".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["a".into(), "b".into()],
        blocks: vec![AudioBlock {
            id: BlockId(INSERT.into()),
            enabled: false,
            kind: AudioBlockKind::Insert(InsertBlock {
                model: "standard".into(),
                io: "fx".into(),
            }),
        }],
        di_output: None,
        loopers: vec![],
    };
    let runtime = build(&chain, &registry);

    let mut input = vec![0.0_f32; FRAMES * DEV_CHANNELS];
    for frame in input.chunks_mut(DEV_CHANNELS) {
        frame[0] = 0.2;
        frame[1] = 0.2;
    }
    let mut tail = vec![0.0_f32; FRAMES * DEV_CHANNELS];
    for _ in 0..256 {
        process_input_f32(&runtime, 0, &input, DEV_CHANNELS);
        tail.fill(0.0);
        process_output_f32(&runtime, 0, &mut tail, DEV_CHANNELS);
    }
    let heard: Vec<f32> = tail.chunks(DEV_CHANNELS).map(|f| f[0]).collect();

    assert!(
        close(level(&heard), 0.4),
        "both heads (0.2 + 0.2) must reach the tail through the bypassed loop, got {}",
        level(&heard)
    );
}

/// A live edit that updates the runtime in place (a VST3 chain, the JACK sync)
/// must keep the loop bypassed — and a footswitch after it must still work.
#[test]
fn an_in_place_update_keeps_the_insert_bypassed() {
    let registry = insert_registry();
    let chain = loop_chain(false);
    let runtime = build(&chain, &registry);
    settle(&runtime, GEAR);

    crate::runtime::update_chain_runtime_state(
        &runtime,
        &chain,
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("the in-place update must apply");
    let (tail, _) = settle(&runtime, GEAR);
    assert!(
        close(level(&tail), GUITAR),
        "after an in-place update the loop is still off — got {}",
        level(&tail)
    );

    toggle(&runtime, true);
    let (tail, _) = settle(&runtime, GEAR);
    assert!(
        close(level(&tail), GEAR),
        "the footswitch still reaches the loop after the update — got {}",
        level(&tail)
    );
}

/// A scene or preset that flips only the insert reaches a runtime updated in
/// place too.
#[test]
fn an_in_place_update_applies_the_inserts_new_state() {
    let registry = insert_registry();
    let runtime = build(&loop_chain(true), &registry);
    settle(&runtime, GEAR);

    crate::runtime::update_chain_runtime_state(
        &runtime,
        &loop_chain(false),
        48_000.0,
        false,
        &[DEFAULT_ELASTIC_TARGET],
        &registry,
    )
    .expect("the in-place update must apply");
    let (tail, _) = settle(&runtime, GEAR);

    assert!(
        close(level(&tail), GUITAR),
        "the update switched the loop off — got {}",
        level(&tail)
    );
}

/// An insert whose E/S does not resolve is a pass-through either way; switching
/// it is a no-op, not an error the owner sees (and not an allocation on the
/// audio thread to build that error).
#[test]
fn switching_an_unbound_insert_is_a_silent_no_op() {
    let mut chain = loop_chain(true);
    for block in chain.blocks.iter_mut() {
        if let AudioBlockKind::Insert(insert) = &mut block.kind {
            insert.io = "not-on-this-machine".into();
        }
    }
    let runtime = build(&chain, &insert_registry());
    settle(&runtime, GEAR);

    toggle(&runtime, false);
    settle(&runtime, GEAR);

    assert_eq!(no_errors(&runtime), Vec::<String>::new());
}

/// The loop's interface runs on its own clock: here the guitar's stream
/// delivers 130 frames per period and the return's 128. Whatever the dry path
/// holds must stay bounded instead of creeping up to the bridge's capacity.
#[test]
fn a_bypassed_insert_on_another_clock_keeps_a_bounded_delay() {
    let runtime = build(&loop_chain(false), &insert_registry());
    for _ in 0..4_000 {
        period_sized(&runtime, GEAR, FRAMES + 2, FRAMES);
    }
    let held = runtime
        .processing
        .lock()
        .expect("processing lock")
        .insert_bridges[0]
        .held_frames();
    assert!(
        held <= 4 * (FRAMES + 2),
        "the dry path must not accumulate the clock difference — it holds {held} frames"
    );
}
