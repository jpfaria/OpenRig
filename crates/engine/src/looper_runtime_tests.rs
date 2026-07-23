//! Issue #323 — the looper wired into the audio callback.
//!
//! Same offline pattern as `di_loop_injection_tests`: build a passthrough
//! runtime, drive `process_input_f32` / `process_output_f32` by hand, and
//! assert on the samples that come out.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use crate::looper::LooperState;
use crate::looper_bank::LooperOp;
use domain::ids::{ChainId, DeviceId};
use project::chain::Chain;
use std::sync::Arc;

const SR: u32 = 48_000;
const UID: u64 = 7;

fn passthrough_runtime(id: &str) -> Arc<super::ChainRuntimeState> {
    let registry = vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let chain = Chain {
        id: ChainId(id.into()),
        description: Some("looper test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
    };
    Arc::new(
        build_chain_runtime_state(&chain, SR as f32, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("passthrough runtime should build"),
    )
}

/// A layer buffer the control thread would allocate before arming a record.
fn layer(runtime: &Arc<super::ChainRuntimeState>) -> Box<[f32]> {
    vec![0.0f32; runtime.looper_max_frames() * 2].into_boxed_slice()
}

fn callback(runtime: &Arc<super::ChainRuntimeState>, level: f32, frames: usize) -> f32 {
    let channels = 2usize;
    let device_in = vec![level; frames * channels];
    process_input_f32(runtime, 0, &device_in, channels);
    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(runtime, 0, &mut out, channels);
    out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[test]
fn creating_a_looper_publishes_an_empty_status() {
    let runtime = passthrough_runtime("looper-create");
    runtime
        .push_looper_op(LooperOp::Create { uid: UID })
        .expect("queue accepts the op");
    callback(&runtime, 0.0, 128);

    let status = runtime.looper_status(UID).expect("looper exists");
    assert_eq!(status.state, LooperState::Empty);
    assert_eq!(status.len_frames, 0);
    assert_eq!(status.layers, 0);
}

#[test]
fn looper_records_the_dry_input_and_plays_it_back() {
    let runtime = passthrough_runtime("looper-record");
    runtime.push_looper_op(LooperOp::Create { uid: UID }).unwrap();
    runtime
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(layer(&runtime)),
        })
        .unwrap();

    // Record two callbacks of a steady 0.5 signal, then close the loop.
    callback(&runtime, 0.5, 128);
    callback(&runtime, 0.5, 128);
    runtime
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: None,
        })
        .unwrap();
    callback(&runtime, 0.0, 128);

    let status = runtime.looper_status(UID).expect("looper exists");
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 256, "two 128-frame callbacks were captured");

    // With a silent device input, the output is the recorded loop.
    let peak = callback(&runtime, 0.0, 128);
    assert!(
        peak > 0.1,
        "the recorded loop did not reach the output (peak {peak})"
    );
}

#[test]
fn a_chain_without_loopers_is_byte_identical_silence() {
    let runtime = passthrough_runtime("looper-absent");
    let peak = callback(&runtime, 0.0, 128);
    assert!(peak < 1e-6, "expected silence, got peak {peak}");
}

#[test]
fn a_playing_looper_never_reaches_another_runtime() {
    let looping = passthrough_runtime("looper-a");
    let quiet = passthrough_runtime("looper-b");

    looping
        .push_looper_op(LooperOp::Create { uid: UID })
        .unwrap();
    looping
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(layer(&looping)),
        })
        .unwrap();
    callback(&looping, 0.5, 128);
    looping
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: None,
        })
        .unwrap();
    callback(&looping, 0.0, 128);
    assert!(callback(&looping, 0.0, 128) > 0.1, "chain A must be looping");

    let peak_b = callback(&quiet, 0.0, 128);
    assert!(
        peak_b < 1e-6,
        "chain B must stay silent — a looper belongs to ONE runtime (peak {peak_b})"
    );
}

#[test]
fn undo_and_clear_hand_the_buffers_back_for_off_thread_drop() {
    let runtime = passthrough_runtime("looper-retire");
    runtime.push_looper_op(LooperOp::Create { uid: UID }).unwrap();
    runtime
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(layer(&runtime)),
        })
        .unwrap();
    callback(&runtime, 0.5, 128);
    runtime
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: None,
        })
        .unwrap();
    callback(&runtime, 0.0, 128);

    runtime.push_looper_op(LooperOp::Clear { uid: UID }).unwrap();
    callback(&runtime, 0.0, 128);

    assert_eq!(
        runtime.drain_retired_layers().len(),
        1,
        "the cleared layer must come back to the control thread"
    );
    let status = runtime.looper_status(UID).expect("looper still exists");
    assert_eq!(status.state, LooperState::Empty);
}

#[test]
fn an_op_for_an_unknown_looper_returns_its_buffer() {
    let runtime = passthrough_runtime("looper-unknown");
    runtime
        .push_looper_op(LooperOp::TapRecord {
            uid: 999,
            buffer: Some(layer(&runtime)),
        })
        .unwrap();
    callback(&runtime, 0.0, 128);

    assert_eq!(
        runtime.drain_retired_layers().len(),
        1,
        "a buffer for a looper that does not exist must not be dropped on the audio thread"
    );
}

#[test]
fn a_recorded_loop_survives_a_runtime_rebuild() {
    let old = passthrough_runtime("looper-swap");
    old.push_looper_op(LooperOp::Create { uid: UID }).unwrap();
    old.push_looper_op(LooperOp::TapRecord {
        uid: UID,
        buffer: Some(layer(&old)),
    })
    .unwrap();
    callback(&old, 0.5, 128);
    old.push_looper_op(LooperOp::TapRecord {
        uid: UID,
        buffer: None,
    })
    .unwrap();
    callback(&old, 0.0, 128);

    let new = passthrough_runtime("looper-swap");
    new.adopt_taps_from(&old);

    let status = new.looper_status(UID).expect("the looper moved over");
    assert_eq!(status.state, LooperState::Playing);
    assert_eq!(status.len_frames, 128);
    assert!(
        callback(&new, 0.0, 128) > 0.1,
        "the adopted loop must keep playing after the rebuild"
    );
}

#[test]
fn removing_a_looper_frees_the_slot_and_its_layers() {
    let runtime = passthrough_runtime("looper-remove");
    runtime.push_looper_op(LooperOp::Create { uid: UID }).unwrap();
    runtime
        .push_looper_op(LooperOp::TapRecord {
            uid: UID,
            buffer: Some(layer(&runtime)),
        })
        .unwrap();
    callback(&runtime, 0.5, 128);
    runtime.push_looper_op(LooperOp::Remove { uid: UID }).unwrap();
    callback(&runtime, 0.0, 128);

    assert!(runtime.looper_status(UID).is_none(), "the slot is free again");
    assert_eq!(runtime.drain_retired_layers().len(), 1);
}
