//! Engine runtime tests (issue #792 split from runtime_tests.rs).
//! Grouped by responsibility; shared fixtures live in `runtime_tests.rs`.
#![allow(unused_imports)]
use super::*;
use super::tests::*;


// ── AudioFrame tests ─────────────────────────────────────────────────────

#[test]
fn audio_frame_mono_mix_mono_returns_sample() {
    let frame = AudioFrame::Mono(0.75);
    assert!((frame.mono_mix() - 0.75).abs() < 1e-6);
}


#[test]
fn audio_frame_mono_mix_stereo_returns_average() {
    let frame = AudioFrame::Stereo([0.4, 0.8]);
    assert!((frame.mono_mix() - 0.6).abs() < 1e-6);
}


// ── blend_frame tests ────────────────────────────────────────────────────

#[test]
fn blend_frame_mono_interpolates_correctly() {
    use super::blend_frame;
    let mut wet = AudioFrame::Mono(1.0);
    let dry = AudioFrame::Mono(0.0);
    blend_frame(&mut wet, dry, 0.5, 0.5);
    assert!((wet.mono_mix() - 0.5).abs() < 1e-6);
}


#[test]
fn blend_frame_stereo_interpolates_correctly() {
    use super::blend_frame;
    let mut wet = AudioFrame::Stereo([1.0, 0.0]);
    let dry = AudioFrame::Stereo([0.0, 1.0]);
    blend_frame(&mut wet, dry, 0.5, 0.5);
    match wet {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.5).abs() < 1e-6);
            assert!((r - 0.5).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}


#[test]
fn blend_frame_layout_mismatch_passes_dry_through() {
    use super::blend_frame;
    let mut wet = AudioFrame::Mono(1.0);
    let dry = AudioFrame::Stereo([0.3, 0.7]);
    blend_frame(&mut wet, dry, 0.5, 0.5);
    // On layout mismatch, frame should be set to dry
    match wet {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.3).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo from dry passthrough"),
    }
}


// ── mix_frames tests ─────────────────────────────────────────────────────

#[test]
fn mix_frames_mono_mono_sums() {
    use super::mix_frames;
    let result = mix_frames(AudioFrame::Mono(0.3), AudioFrame::Mono(0.5));
    assert!(matches!(result, AudioFrame::Mono(v) if (v - 0.8).abs() < 1e-6));
}


#[test]
fn mix_frames_stereo_stereo_sums() {
    use super::mix_frames;
    let result = mix_frames(
        AudioFrame::Stereo([0.1, 0.2]),
        AudioFrame::Stereo([0.3, 0.4]),
    );
    match result {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.4).abs() < 1e-6);
            assert!((r - 0.6).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}


#[test]
fn mix_frames_mono_stereo_widens() {
    use super::mix_frames;
    let result = mix_frames(AudioFrame::Mono(0.5), AudioFrame::Stereo([0.1, 0.2]));
    match result {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.6).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}


#[test]
fn mix_frames_stereo_mono_widens() {
    use super::mix_frames;
    let result = mix_frames(AudioFrame::Stereo([0.1, 0.2]), AudioFrame::Mono(0.5));
    match result {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.6).abs() < 1e-6);
            assert!((r - 0.7).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}


// ── output_limiter tests ─────────────────────────────────────────────────

#[test]
fn output_limiter_transparent_below_threshold() {
    use super::output_limiter;
    assert!((output_limiter(0.5) - 0.5).abs() < 1e-6);
    assert!((output_limiter(-0.5) - (-0.5)).abs() < 1e-6);
    assert!((output_limiter(0.0) - 0.0).abs() < 1e-6);
    assert!((output_limiter(0.94) - 0.94).abs() < 1e-6);
}


#[test]
fn output_limiter_saturates_above_threshold() {
    // Issue #496: the previous tanh form was discontinuous (-2.17 dB
    // step at the threshold) and non-monotonic from ~0.95 to ~1.83
    // (proven RED in runtime_dsp::tests). Pin the PROPERTY a soft
    // limiter must have — bounded, sign-preserving, smaller than the
    // input above the knee — not the specific tanh function.
    use super::output_limiter;
    let limited = output_limiter(2.0);
    assert!(
        limited < 2.0 && limited > 0.0,
        "reduce + keep sign: {limited}"
    );
    assert!(limited <= 1.0 && limited.is_finite(), "bounded: {limited}");
}


#[test]
fn output_limiter_negative_saturates_symmetrically() {
    // Issue #496: assert odd symmetry instead of pinning tanh()
    // numerically. The new soft-clip is not tanh but is still odd,
    // bounded, monotonic and continuous.
    use super::output_limiter;
    assert!((output_limiter(-2.0) + output_limiter(2.0)).abs() < 1e-6);
    assert!(output_limiter(-2.0) >= -1.0 && output_limiter(-2.0).is_finite());
}


// ── apply_mixdown tests ──────────────────────────────────────────────────

#[test]
fn apply_mixdown_sum_adds_channels() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Sum, 0.3, 0.5) - 0.8).abs() < 1e-6);
}


#[test]
fn apply_mixdown_average_averages_channels() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Average, 0.4, 0.8) - 0.6).abs() < 1e-6);
}


#[test]
fn apply_mixdown_left_returns_left() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Left, 0.3, 0.7) - 0.3).abs() < 1e-6);
}


#[test]
fn apply_mixdown_right_returns_right() {
    use super::apply_mixdown;
    use project::chain::ChainOutputMixdown;
    assert!((apply_mixdown(ChainOutputMixdown::Right, 0.3, 0.7) - 0.7).abs() < 1e-6);
}


// ── layout_from_channels tests ───────────────────────────────────────────

#[test]
fn layout_from_channels_mono_ok() {
    use super::layout_from_channels;
    assert_eq!(layout_from_channels(1).unwrap(), AudioChannelLayout::Mono);
}


#[test]
fn layout_from_channels_stereo_ok() {
    use super::layout_from_channels;
    assert_eq!(layout_from_channels(2).unwrap(), AudioChannelLayout::Stereo);
}


#[test]
fn layout_from_channels_invalid_errors() {
    use super::layout_from_channels;
    assert!(layout_from_channels(0).is_err());
    assert!(layout_from_channels(3).is_err());
    assert!(layout_from_channels(8).is_err());
}


// ── write_output_frame tests ─────────────────────────────────────────────

#[test]
fn write_output_frame_mono_to_single_channel() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 2];
    write_output_frame(
        AudioFrame::Mono(0.5),
        &[1],
        &mut frame,
        ChainOutputMixdown::Average,
    );
    assert!(
        (frame[0] - 0.0).abs() < 1e-6,
        "channel 0 should be untouched"
    );
    assert!(
        (frame[1] - 0.5).abs() < 1e-6,
        "channel 1 should have the sample"
    );
}


#[test]
fn write_output_frame_mono_to_multiple_channels() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 4];
    write_output_frame(
        AudioFrame::Mono(0.8),
        &[0, 2, 3],
        &mut frame,
        ChainOutputMixdown::Average,
    );
    assert!((frame[0] - 0.8).abs() < 1e-6);
    assert!((frame[1] - 0.0).abs() < 1e-6);
    assert!((frame[2] - 0.8).abs() < 1e-6);
    assert!((frame[3] - 0.8).abs() < 1e-6);
}


#[test]
fn write_output_frame_stereo_to_zero_channels() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 2];
    // Empty channels — should not write anything
    write_output_frame(
        AudioFrame::Stereo([0.5, 0.7]),
        &[],
        &mut frame,
        ChainOutputMixdown::Average,
    );
    assert_eq!(frame, [0.0, 0.0]);
}


#[test]
fn write_output_frame_stereo_to_one_channel_uses_mixdown() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 2];
    write_output_frame(
        AudioFrame::Stereo([0.4, 0.8]),
        &[0],
        &mut frame,
        ChainOutputMixdown::Average,
    );
    // Average of 0.4 and 0.8 = 0.6
    assert!((frame[0] - 0.6).abs() < 1e-6);
}


#[test]
fn write_output_frame_stereo_to_two_channels_preserves_lr() {
    use super::write_output_frame;
    use project::chain::ChainOutputMixdown;
    let mut frame = [0.0f32; 4];
    write_output_frame(
        AudioFrame::Stereo([0.3, 0.7]),
        &[1, 3],
        &mut frame,
        ChainOutputMixdown::Average,
    );
    assert!((frame[0] - 0.0).abs() < 1e-6);
    assert!((frame[1] - 0.3).abs() < 1e-6);
    assert!((frame[2] - 0.0).abs() < 1e-6);
    assert!((frame[3] - 0.7).abs() < 1e-6);
}


// ── read_input_frame tests ───────────────────────────────────────────────

#[test]
fn read_input_frame_mono_reads_correct_channel() {
    use super::read_input_frame;
    let data = [0.1, 0.9, 0.5, 0.3];
    let frame = read_input_frame(AudioChannelLayout::Mono, &[2], &data);
    assert!(matches!(frame, AudioFrame::Mono(v) if (v - 0.5).abs() < 1e-6));
}


#[test]
fn read_input_frame_stereo_reads_two_channels() {
    use super::read_input_frame;
    let data = [0.1, 0.2, 0.3, 0.4];
    let frame = read_input_frame(AudioChannelLayout::Stereo, &[1, 3], &data);
    match frame {
        AudioFrame::Stereo([l, r]) => {
            assert!((l - 0.2).abs() < 1e-6);
            assert!((r - 0.4).abs() < 1e-6);
        }
        _ => panic!("expected stereo"),
    }
}


#[test]
fn read_input_frame_out_of_bounds_returns_zero() {
    use super::read_input_frame;
    let data = [0.5f32; 2];
    let frame = read_input_frame(AudioChannelLayout::Mono, &[99], &data);
    assert!(matches!(frame, AudioFrame::Mono(v) if v.abs() < 1e-6));
}


// ── silent_frame tests ───────────────────────────────────────────────────

#[test]
fn silent_frame_mono_is_zero() {
    use super::silent_frame;
    let frame = silent_frame(AudioChannelLayout::Mono);
    assert!(matches!(frame, AudioFrame::Mono(v) if v.abs() < 1e-6));
}


#[test]
fn silent_frame_stereo_is_zero() {
    use super::silent_frame;
    let frame = silent_frame(AudioChannelLayout::Stereo);
    assert!(matches!(frame, AudioFrame::Stereo([l, r]) if l.abs() < 1e-6 && r.abs() < 1e-6));
}


// ── layout_label tests ───────────────────────────────────────────────────

#[test]
fn layout_label_returns_correct_strings() {
    use super::layout_label;
    assert_eq!(layout_label(AudioChannelLayout::Mono), "mono");
    assert_eq!(layout_label(AudioChannelLayout::Stereo), "stereo");
}


// ── read_channel edge cases ─────────────────────────────────────────────

#[test]
fn read_channel_valid_index() {
    use super::read_channel;
    let data = [0.1, 0.2, 0.3];
    assert!((read_channel(&data, 1) - 0.2).abs() < 1e-6);
}


#[test]
fn read_channel_out_of_bounds_returns_zero() {
    use super::read_channel;
    let data = [0.5, 0.7];
    assert!((read_channel(&data, 10)).abs() < 1e-6);
}


#[test]
fn read_channel_empty_data_returns_zero() {
    use super::read_channel;
    let data: [f32; 0] = [];
    assert!((read_channel(&data, 0)).abs() < 1e-6);
}

