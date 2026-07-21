//! Multi-channel / split-mono / dual-mono isolation tests (issue #792
//! split from stream_isolation_tests.rs). Shares the binding fixtures with
//! the base suite via super::stream_isolation.

use super::stream_isolation::{binding, bound_chain, in_ep, out_ep};
use super::*;
use domain::io_binding::ChannelMode;

/// Two channels of the same device must NOT cancel each other in the
/// output. Send +0.5 on channel 0 and −0.5 on channel 1 of one
/// 2-channel-mono InputBlock; the output of a passthrough chain MUST
/// still carry both signals (in a stereo output) or, if the chain
/// architecture mixes them, the sum MUST not be zero (which would mean
/// total cancellation = total interference).
///
/// This test currently FAILS on the post-revert architecture: both
/// segments upmix Mono→Stereo by broadcasting (Stereo([s, s])), then
/// the engine sums into a single shared output buffer, producing
/// Stereo([s_ch0 + s_ch1, s_ch0 + s_ch1]) — which is silence when the
/// two inputs are equal-and-opposite. That is the user-visible "channel
/// 2 interfering with channel 1" bug, exposed mathematically.
///
/// When we fix the architecture so each split-mono segment writes only
/// to its own output channel position, this test PASSES: ch0 in left,
/// ch1 in right, both preserved.
#[test]
#[ignore = "PENDING #350 phase 2 — current arch broadcasts mono and sums, cancelling opposite-phase signals"]
fn two_channel_mono_input_must_not_cancel_in_output() {
    use crate::runtime::{process_input_f32, process_output_f32};

    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0, 1])],
        vec![out_ep("out0", "monitor", ChannelMode::Stereo, vec![0, 1])],
    );
    let chain = bound_chain("isolation:no-cancel", None, vec![]);

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry)
            .expect("passthrough chain must build"),
    );

    let frames = 64usize;
    // Stereo interleaved: ch0 = +0.5 every frame, ch1 = −0.5 every frame.
    let data: Vec<f32> = (0..frames).flat_map(|_| [0.5_f32, -0.5_f32]).collect();

    // Fire the cpal callback for input_index 0 (the only one — both
    // segments share the index since they hit the same physical device).
    process_input_f32(&runtime, 0, &data, 2);

    // Drain the output: stereo, 2 channels.
    let mut out = vec![0.0_f32; frames * 2];
    process_output_f32(&runtime, 0, &mut out, 2);

    // Energy invariant: at least ONE of the two output channels must
    // carry a non-trivial signal. If BOTH channels are below the noise
    // floor, the signals cancelled — total interference.
    let abs_energy_left: f32 = out.iter().step_by(2).map(|s| s.abs()).sum();
    let abs_energy_right: f32 = out.iter().skip(1).step_by(2).map(|s| s.abs()).sum();
    let total_energy = abs_energy_left + abs_energy_right;
    assert!(
        total_energy > 1e-3,
        "output channels are silent (left={:.6}, right={:.6}) — the two input \
         signals cancelled each other. Streams are not isolated.",
        abs_energy_left,
        abs_energy_right
    );
}

/// Inverse of the cancellation test: send +0.5 on BOTH channels and
/// verify the output is NOT saturated (above 0.95) by the sum (1.0)
/// hitting tanh saturation. If isolated, each channel keeps its own
/// 0.5 signal in its own output channel — no clipping. If summed
/// architecture, both output channels carry tanh(1.0) ≈ 0.76, audible
/// as soft distortion.
#[test]
#[ignore = "PENDING #350 phase 2 — same-phase signals must be carried separately, not summed and limited"]
fn two_channel_mono_input_must_not_saturate_when_both_loud() {
    use crate::runtime::{process_input_f32, process_output_f32};

    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0, 1])],
        vec![out_ep("out0", "monitor", ChannelMode::Stereo, vec![0, 1])],
    );
    let chain = bound_chain("isolation:no-sat", None, vec![]);

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry)
            .expect("passthrough chain must build"),
    );

    let frames = 64usize;
    // Both channels at +0.5 (well below clip if isolated).
    let data: Vec<f32> = (0..frames).flat_map(|_| [0.5_f32, 0.5_f32]).collect();

    process_input_f32(&runtime, 0, &data, 2);

    let mut out = vec![0.0_f32; frames * 2];
    process_output_f32(&runtime, 0, &mut out, 2);

    // Each output channel must carry ~0.5 (its own input channel) — NOT
    // tanh(1.0)≈0.76 from summed-and-limited mixing.
    for (i, &sample) in out.iter().enumerate() {
        let channel = i % 2;
        assert!(
            (sample - 0.5).abs() < 0.05,
            "out[{}] (channel {}) = {:.4}; expected ~0.5 (the matching input \
             channel). A larger value means the engine summed two channels \
             and the limiter saturated — streams not isolated.",
            i,
            channel,
            sample
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// "Stream is ALWAYS stereo internally" invariant (issue #350).
// ─────────────────────────────────────────────────────────────────────────
//
// Project rule (CLAUDE.md non-regression invariant #5): every internal
// stream processes on a STEREO bus when the chain output is stereo —
// regardless of input mode. Mono input upmixes by broadcasting
// (Stereo([s, s])); two split-mono siblings are TWO separate stereo
// streams (each broadcast), summed at fan-out with 1/N gain to avoid
// limiter saturation. Auto-panning, forcing Mono bus on a stereo
// chain, or sending one guitar to one ear is FORBIDDEN.
//
// The tests below pin the rule. Reintroducing the Mono-bus override or
// auto-pan will break them.

/// `processing_layout` of every InputProcessingState in a chain whose
/// OutputBlock is stereo MUST be Stereo — split-mono siblings included.
/// This catches the regression where a previous fix forced Mono bus
/// for split-mono segments and the user heard each guitar in only one
/// ear (auto-pan effect via partial broadcast).
#[test]
fn split_mono_segments_keep_stereo_processing_when_output_is_stereo() {
    // 1 InputBlock, 2 channels, mono mode → 2 effective entries (one per
    // channel) — the user's "duas guitarras na mesma input" config.
    // Stereo output → bus must stay stereo for every stream.
    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0, 1])],
        vec![out_ep("out0", "monitor", ChannelMode::Stereo, vec![0, 1])],
    );
    let chain = bound_chain("isolation:always-stereo", None, vec![]);

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry)
            .expect("split-mono / stereo-output chain must build"),
    );
    let processing = runtime.processing.lock().expect("lock poisoned");

    assert!(
        processing.input_states.len() >= 2,
        "fixture invariant: split-mono with 2 channels must produce ≥2 segments, got {}",
        processing.input_states.len()
    );

    for (i, state) in processing.input_states.iter().enumerate() {
        assert!(
            matches!(state.processing_layout, AudioChannelLayout::Stereo),
            "segment {} processing_layout = {:?}; must be Stereo when chain \
             output is stereo, even for split-mono entries. Forcing Mono bus \
             here breaks the 'stream is always stereo internally' rule and \
             produces auto-pan / one-ear-only output.",
            i,
            state.processing_layout
        );
    }
}

/// DualMono input + stereo output: also Stereo bus (the DualMono variant
/// is flattened to a Stereo layout at the buffer level; L/R independence
/// is preserved internally by `AudioProcessor::DualMono`).
#[test]
fn dual_mono_segment_keeps_stereo_processing() {
    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::DualMono, vec![0, 1])],
        vec![out_ep("out0", "monitor", ChannelMode::Stereo, vec![0, 1])],
    );
    let chain = bound_chain("isolation:dualmono-stereo", None, vec![]);

    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry)
            .expect("dualmono chain must build"),
    );
    let processing = runtime.processing.lock().expect("lock poisoned");

    for (i, state) in processing.input_states.iter().enumerate() {
        assert!(
            matches!(state.processing_layout, AudioChannelLayout::Stereo),
            "DualMono segment {} processing_layout = {:?}; must be Stereo. \
             DualMono is flattened to a Stereo bus at the buffer level, with \
             internal L/R independence preserved by AudioProcessor::DualMono.",
            i,
            state.processing_layout
        );
    }
}

/// Mono input + Mono OUTPUT: Mono bus is correct. The "always stereo"
/// rule applies WHEN OUTPUT IS STEREO. If the user explicitly configures
/// a mono output, we don't force a useless upmix.
#[test]
fn mono_input_with_mono_output_stays_mono() {
    let registry = binding(
        vec![in_ep("in0", "scarlett", ChannelMode::Mono, vec![0])],
        vec![out_ep("out0", "monitor", ChannelMode::Mono, vec![0])],
    );
    let chain = bound_chain("isolation:mono-mono", None, vec![]);
    let runtime = std::sync::Arc::new(
        build_chain_runtime_state(&chain, 48_000.0, &[256], &registry)
            .expect("mono-only chain must build"),
    );
    let processing = runtime.processing.lock().expect("lock poisoned");

    for (i, state) in processing.input_states.iter().enumerate() {
        assert!(
            matches!(state.processing_layout, AudioChannelLayout::Mono),
            "segment {} processing_layout = {:?}; mono in + mono out must \
             stay Mono (no useless upmix).",
            i,
            state.processing_layout
        );
    }
}
