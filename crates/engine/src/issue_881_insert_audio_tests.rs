//! #881 — an `Insert` block must never silence the chain.
//!
//! The reported bug is audible: adding an insert made the signal disappear. It
//! was not "no route" — the graph build indexed past the effective inputs and
//! panicked, so the chain had no runtime at all. These drive real buffers
//! through the built runtime and assert the guitar still comes out the tail.

use std::sync::Arc;

use super::tests::{insert_chain, insert_registry};
use super::{build_chain_runtime_state, process_input_f32, process_output_f32};
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use project::block::AudioBlockKind;
use project::chain::Chain;

const FRAMES: usize = 128;
const CHANNELS: usize = 2;

/// Push a steady tone through input 0 and read the tail output, letting the
/// fade-in settle first.
fn peak_through(chain: &Chain) -> f32 {
    let runtime = Arc::new(
        build_chain_runtime_state(
            chain,
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &insert_registry(),
        )
        .expect("the chain must build a runtime"),
    );
    let input = vec![0.5_f32; FRAMES * CHANNELS];
    let mut output = vec![0.0_f32; FRAMES * CHANNELS];
    let mut peak = 0.0_f32;
    for i in 0..256 {
        process_input_f32(&runtime, 0, &input, CHANNELS);
        output.fill(0.0);
        process_output_f32(&runtime, 0, &mut output, CHANNELS);
        // Ignore the fade-in ramp; measure once the chain is running.
        if i >= 128 {
            peak = peak.max(output.iter().fold(0.0_f32, |m, s| m.max(s.abs())));
        }
    }
    peak
}

#[test]
fn an_unbound_insert_lets_the_chain_play() {
    let mut chain = insert_chain();
    let AudioBlockKind::Insert(ref mut ib) = chain.blocks[2].kind else {
        panic!("block 2 should be the insert");
    };
    ib.io = String::new();

    let peak = peak_through(&chain);

    assert!(
        peak > 0.01,
        "an insert with no E/S must be bypassed, not silence the chain — tail peak was {peak}"
    );
}

#[test]
fn an_insert_bound_to_a_missing_binding_lets_the_chain_play() {
    let mut chain = insert_chain();
    let AudioBlockKind::Insert(ref mut ib) = chain.blocks[2].kind else {
        panic!("block 2 should be the insert");
    };
    // A project moved from another machine: the id resolves to nothing here.
    ib.io = "loop_on_the_other_rig".into();

    let peak = peak_through(&chain);

    assert!(
        peak > 0.01,
        "a project whose insert binding is absent on this machine must still \
         play — tail peak was {peak}"
    );
}

#[test]
fn a_disabled_insert_lets_the_chain_play() {
    let mut chain = insert_chain();
    chain.blocks[2].enabled = false;

    let peak = peak_through(&chain);

    assert!(
        peak > 0.01,
        "a disabled insert is a bypass, not a cut — tail peak was {peak}"
    );
}

/// The reported rig, end to end: the SYNERGY loop shares the guitar's interface
/// (HD 8 — send on OUT 5, return on IN 4). One cpal input stream carries the
/// whole device, so the guitar must leave on the SEND route and whatever comes
/// back on the return channel must leave on the TAIL route. With the return
/// pinned to a cpal stream nobody opens, the tail stayed silent — "não sai som".
#[test]
fn a_loop_on_the_guitars_interface_sends_and_returns() {
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};

    const HD8: &str = "coreaudio:hd8";
    const DEVICE_CHANNELS: usize = 8;
    const GUITAR_CH: usize = 0;
    const RETURN_CH: usize = 3; // IN 4
    const SEND_CH: usize = 4; // OUT 5

    let mono = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(HD8.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![
        IoBinding {
            id: "io".into(),
            name: "HD 8 - 1".into(),
            inputs: vec![mono("in0", GUITAR_CH)],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId(HD8.into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
        IoBinding {
            id: "fx".into(),
            name: "SYNERGY".into(),
            inputs: vec![mono("ret", RETURN_CH)],
            outputs: vec![mono("snd", SEND_CH)],
        },
    ];

    let runtime = Arc::new(
        build_chain_runtime_state(
            &insert_chain(),
            48_000.0,
            &[DEFAULT_ELASTIC_TARGET],
            &registry,
        )
        .expect("the chain must build a runtime"),
    );

    // One interleaved buffer for the whole device: guitar on IN 1, the pedal's
    // output coming back on IN 4.
    let mut input = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    for frame in input.chunks_exact_mut(DEVICE_CHANNELS) {
        frame[GUITAR_CH] = 0.5;
        frame[RETURN_CH] = 0.25;
    }
    let mut send = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    let mut tail = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
    let (mut send_peak, mut tail_peak) = (0.0_f32, 0.0_f32);
    for i in 0..256 {
        process_input_f32(&runtime, 0, &input, DEVICE_CHANNELS);
        send.fill(0.0);
        tail.fill(0.0);
        process_output_f32(&runtime, 1, &mut send, DEVICE_CHANNELS);
        process_output_f32(&runtime, 0, &mut tail, DEVICE_CHANNELS);
        if i >= 128 {
            for frame in send.chunks_exact(DEVICE_CHANNELS) {
                send_peak = send_peak.max(frame[SEND_CH].abs());
            }
            for frame in tail.chunks_exact(DEVICE_CHANNELS) {
                tail_peak = tail_peak.max(frame[0].abs());
            }
        }
    }

    assert!(
        send_peak > 0.01,
        "the guitar must leave on OUT 5 for the pedal — send peak was {send_peak}"
    );
    assert!(
        tail_peak > 0.01,
        "what comes back on IN 4 must leave on the chain's tail output — tail \
         peak was {tail_peak}"
    );
}

/// A block BEFORE the insert must colour what leaves on the SEND — that is the
/// whole point of putting a pedal in front of the loop. Reported after the
/// routing fix: "coloquei o pedal de ganho antes do insert e não altera o
/// timbre".
#[test]
fn a_block_before_the_insert_shapes_the_send() {
    use domain::ids::DeviceId;
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use domain::value_objects::ParameterValue;
    use project::param::ParameterSet;

    const HD8: &str = "coreaudio:hd8";
    const DEVICE_CHANNELS: usize = 8;
    const SEND_CH: usize = 4;

    let mono = |name: &str, ch: usize| IoEndpoint {
        name: name.into(),
        device_id: DeviceId(HD8.into()),
        mode: ChannelMode::Mono,
        channels: vec![ch],
    };
    let registry = vec![
        IoBinding {
            id: "io".into(),
            name: "HD 8 - 1".into(),
            inputs: vec![mono("in0", 0)],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId(HD8.into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        },
        IoBinding {
            id: "fx".into(),
            name: "SYNERGY".into(),
            inputs: vec![mono("ret", 3)],
            outputs: vec![mono("snd", SEND_CH)],
        },
    ];

    // `insert_chain` is [input, gain:volume, insert, gain:volume, output] — the
    // block at index 1 is the pedal in FRONT of the loop.
    let send_peak_at = |volume: f32| {
        let mut chain = insert_chain();
        let AudioBlockKind::Core(ref mut cb) = chain.blocks[1].kind else {
            panic!("block 1 should be the gain in front of the insert");
        };
        let mut params = ParameterSet::default();
        params.insert("volume", ParameterValue::Float(volume));
        cb.params = params;

        let runtime = Arc::new(
            build_chain_runtime_state(&chain, 48_000.0, &[DEFAULT_ELASTIC_TARGET], &registry)
                .expect("the chain must build a runtime"),
        );
        let mut input = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
        for frame in input.chunks_exact_mut(DEVICE_CHANNELS) {
            frame[0] = 0.5;
        }
        let mut send = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
        let mut peak = 0.0_f32;
        for i in 0..256 {
            process_input_f32(&runtime, 0, &input, DEVICE_CHANNELS);
            send.fill(0.0);
            process_output_f32(&runtime, 1, &mut send, DEVICE_CHANNELS);
            if i >= 128 {
                for frame in send.chunks_exact(DEVICE_CHANNELS) {
                    peak = peak.max(frame[SEND_CH].abs());
                }
            }
        }
        peak
    };

    let quiet = send_peak_at(10.0);
    let loud = send_peak_at(100.0);

    assert!(
        loud > quiet * 1.5,
        "the pedal in front of the loop must shape what goes out the send — \
         volume 10 gave {quiet}, volume 100 gave {loud}"
    );
}
