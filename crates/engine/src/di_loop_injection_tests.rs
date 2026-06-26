//! Tests for per-chain virtual DI loop injection (issue #614).
//!
//! These tests verify that `process_input_f32` correctly substitutes the DI
//! loop buffer for device input when a loop is published, that the device
//! passthrough is byte-identical when no loop is active, and that the
//! playback cursor advances and wraps as expected.

use super::{
    build_chain_runtime_state, process_input_f32, process_output_f32, DEFAULT_ELASTIC_TARGET,
};
use crate::di_loop::DiLoop;
use domain::ids::{ChainId, DeviceId};
use project::chain::Chain;
use std::sync::atomic::Ordering;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

const SR: u32 = 48_000;

/// Build a minimal stereo passthrough chain runtime — same pattern as
/// `audio_signal_integrity_tests::build_runtime`.
fn passthrough_runtime() -> Arc<super::ChainRuntimeState> {
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
        id: ChainId("di-test".into()),
        description: Some("DI injection test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    };
    Arc::new(
        build_chain_runtime_state(&chain, SR as f32, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("passthrough runtime should build"),
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// When a DI loop carrying non-zero samples is published, feeding silent device
/// input must produce non-silent output (the DI samples reached the chain).
#[test]
fn di_loop_replaces_silent_device_input() {
    let runtime = passthrough_runtime();
    // Build a 256-sample mono loop at 0.5 amplitude.
    let di = Arc::new(DiLoop::from_samples(&[0.5; 256], SR, 1, SR, 0));
    runtime.set_di_loop(Some(di));

    let (frames, channels) = (128usize, 2usize);
    let device_in = vec![0.0f32; frames * channels];

    process_input_f32(&runtime, 0, &device_in, channels);
    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(&runtime, 0, &mut out, channels);

    let peak = out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        peak > 0.1,
        "DI loop did not reach output (peak {peak}); device_in was silent, expected DI samples"
    );
}

/// When no DI loop is active, silent device input must produce silent output
/// (byte-identical to today's passthrough — invariant #9).
#[test]
fn off_is_silent_passthrough_of_device() {
    let runtime = passthrough_runtime();
    // No DI loop installed.

    let (frames, channels) = (128usize, 2usize);
    let device_in = vec![0.0f32; frames * channels];

    process_input_f32(&runtime, 0, &device_in, channels);
    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(&runtime, 0, &mut out, channels);

    let peak = out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        peak < 1e-6,
        "expected silence with no DI loop, got peak {peak}"
    );
}

/// The playback cursor must advance by `num_frames` each callback and wrap
/// modulo the loop length.
#[test]
fn cursor_advances_by_num_frames_and_wraps() {
    let runtime = passthrough_runtime();
    // 200-frame loop so wrapping is observable with 128-frame callbacks.
    let di = Arc::new(DiLoop::from_samples(&vec![0.1f32; 200], SR, 1, SR, 0));
    runtime.set_di_loop(Some(di));

    let (frames, channels) = (128usize, 2usize);
    let device_in = vec![0.0f32; frames * channels];

    // First callback: cursor starts at 0, should advance to 128.
    process_input_f32(&runtime, 0, &device_in, channels);
    assert_eq!(
        runtime.di_loop_pos.load(Ordering::Relaxed),
        128,
        "after first callback cursor should be 128"
    );

    // Second callback: 128 + 128 = 256, wrapped mod 200 = 56.
    process_input_f32(&runtime, 0, &device_in, channels);
    assert_eq!(
        runtime.di_loop_pos.load(Ordering::Relaxed),
        56,
        "after second callback cursor should wrap to 56 (256 % 200)"
    );
}

/// Build a passthrough runtime whose InputBlock has TWO mono sources on the
/// same device (ch0 + ch1) — the "two guitars, one chain" rig from issue
/// #699. Both entries become parallel segments inside one runtime.
fn two_source_runtime() -> Arc<super::ChainRuntimeState> {
    let registry = vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![
            domain::io_binding::IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("dev".into()),
                mode: domain::io_binding::ChannelMode::Mono,
                channels: vec![0],
            },
            domain::io_binding::IoEndpoint {
                name: "in1".into(),
                device_id: DeviceId("dev".into()),
                mode: domain::io_binding::ChannelMode::Mono,
                channels: vec![1],
            },
        ],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }];
    let chain = Chain {
        id: ChainId("di-test-multi".into()),
        description: Some("DI multi-source test".into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    };
    Arc::new(
        build_chain_runtime_state(&chain, SR as f32, &[DEFAULT_ELASTIC_TARGET], &registry)
            .expect("two-source runtime should build"),
    )
}

/// Issue #699 — a chain with two input sources must play an armed DI loop
/// exactly ONCE. Today every segment substitutes the loop for its device
/// frames, so the two passthrough copies sum to double the loop amplitude
/// at the output.
#[test]
fn di_loop_plays_once_on_multi_source_chain() {
    let runtime = two_source_runtime();
    let di = Arc::new(DiLoop::from_samples(&[0.5; 256], SR, 1, SR, 0));
    runtime.set_di_loop(Some(di));

    let (frames, channels) = (128usize, 2usize);
    let device_in = vec![0.0f32; frames * channels];

    process_input_f32(&runtime, 0, &device_in, channels);
    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(&runtime, 0, &mut out, channels);

    let peak = out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        (peak - 0.5).abs() < 1e-3,
        "DI loop must play exactly once on a two-source chain: expected peak \
         ~0.5 (one copy), got {peak} (issue #699 — every segment injected the loop)"
    );
}

/// Issue #699 — while the DI loop is armed, the chain's OTHER segments must
/// be silent (DI playback replaces ALL live input, it does not leave the
/// second source bleeding through).
#[test]
fn di_loop_silences_other_segments_live_input() {
    let runtime = two_source_runtime();
    let di = Arc::new(DiLoop::from_samples(&[0.0; 256], SR, 1, SR, 0));
    runtime.set_di_loop(Some(di));

    // Hot live signal on BOTH device channels — with a silent loop armed,
    // none of it may reach the output.
    let (frames, channels) = (128usize, 2usize);
    let device_in = vec![0.8f32; frames * channels];

    process_input_f32(&runtime, 0, &device_in, channels);
    let mut out = vec![0.0f32; frames * channels];
    process_output_f32(&runtime, 0, &mut out, channels);

    let peak = out.iter().cloned().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        peak < 1e-6,
        "live input bled through while a DI loop was armed: peak {peak}"
    );
}
