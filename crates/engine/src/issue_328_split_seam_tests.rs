//! #328 live report (2026-09-30): a Split → Mix chain feeding two outputs
//! (Main `[0,1]` and `[24,25]`) went "bee box" — the Main loopback showed a
//! step at the edge of 73 % of its 64-frame buffers (689 Hz buzz), steady
//! until the chain restarts. A continuous tone through the chain must come
//! out continuous on every output, buffer after buffer.

use super::issue_328_split_mix::{split_block, unit_impulse_ir_block};
use super::volume_invariants::*;

use project::block::SplitEnd;

const FRAMES: usize = 64;
const DEVICE_CHANNELS: usize = 26;
/// Left channel of each output: Main `[0,1]`, then `[24,25]`.
const LEFT: [usize; 2] = [0, 24];

fn tone(frames: usize, start: usize) -> Vec<f32> {
    (start..start + frames)
        .map(|n| 0.5 * (std::f32::consts::TAU * 1_000.0 * n as f32 / SR).sin())
        .collect()
}

fn split_two_output_chain() -> (Chain, Vec<IoBinding>) {
    let (chain, mut registry) = chain_with_blocks(
        "split_two_outputs",
        input_mono(vec![0]),
        vec![
            split_block(
                "split",
                SplitEnd::Mix,
                &[],
                vec![unit_impulse_ir_block("amp_a")],
                vec![unit_impulse_ir_block("amp_b")],
            ),
            unit_impulse_ir_block("reverb"),
        ],
        output(ChannelMode::Stereo, vec![0, 1]),
    );
    let mut second = output(ChannelMode::Stereo, vec![24, 25]);
    second.name = "out-24".into();
    registry[0].outputs.push(second);
    (chain, registry)
}

/// Largest second difference across a buffer edge vs inside the buffers.
fn seam_vs_interior(left: &[f32]) -> (f32, f32) {
    let (mut seam, mut interior) = (0.0_f32, 0.0_f32);
    for n in 1..left.len() - 1 {
        let d2 = (left[n + 1] - 2.0 * left[n] + left[n - 1]).abs();
        if n % FRAMES == 0 || n % FRAMES == FRAMES - 1 {
            seam = seam.max(d2);
        } else {
            interior = interior.max(d2);
        }
    }
    (seam, interior)
}

#[test]
fn a_split_feeding_two_outputs_plays_a_continuous_tone_on_each() {
    let (chain, registry) = split_two_output_chain();
    let runtime = build_runtime(&chain, &registry);
    let mut played = [Vec::new(), Vec::new()];
    for callback in 0..400 {
        process_input_f32(&runtime, 0, &tone(FRAMES, callback * FRAMES), 1);
        for (route, out) in played.iter_mut().enumerate() {
            let mut buf = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
            process_output_f32(&runtime, route, &mut buf, DEVICE_CHANNELS);
            if callback >= 100 {
                out.extend(buf.chunks(DEVICE_CHANNELS).map(|f| f[LEFT[route]]));
            }
        }
    }
    for (route, out) in played.iter().enumerate() {
        let (seam, interior) = seam_vs_interior(out);
        assert!(
            peak_abs(out) > 0.1,
            "route {route} must play the tone, peak {}",
            peak_abs(out)
        );
        assert!(
            seam < 2.0 * interior,
            "route {route}: step at the buffer edge — seam {seam} vs interior {interior}"
        );
    }
}

/// The live report came minutes after path B's amp was swapped on the
/// running chain: after an in-place edit of path B the tone must still come
/// out continuous on every output.
#[test]
fn swapping_path_b_on_a_running_split_keeps_every_output_continuous() {
    let (chain, registry) = split_two_output_chain();
    let runtime = build_runtime(&chain, &registry);
    let mut edited = chain.clone();
    if let project::block::AudioBlockKind::Split(split) = &mut edited.blocks[0].kind {
        split.paths[1] = vec![super::issue_328_split_mix::volume_block("amp_b2", 80.0)];
    }
    let mut played = [Vec::new(), Vec::new()];
    for callback in 0..600 {
        if callback == 100 {
            super::update_chain_runtime_state(
                &runtime,
                &edited,
                SR,
                false,
                &[DEFAULT_ELASTIC_TARGET],
                &registry,
            )
            .expect("in-place edit of path B");
        }
        process_input_f32(&runtime, 0, &tone(FRAMES, callback * FRAMES), 1);
        for (route, out) in played.iter_mut().enumerate() {
            let mut buf = vec![0.0_f32; FRAMES * DEVICE_CHANNELS];
            process_output_f32(&runtime, route, &mut buf, DEVICE_CHANNELS);
            if callback >= 300 {
                out.extend(buf.chunks(DEVICE_CHANNELS).map(|f| f[LEFT[route]]));
            }
        }
    }
    for (route, out) in played.iter().enumerate() {
        let (seam, interior) = seam_vs_interior(out);
        assert!(peak_abs(out) > 0.1, "route {route} must play the tone");
        assert!(
            seam < 2.0 * interior,
            "route {route}: step at the buffer edge after the edit — seam {seam} vs interior {interior}"
        );
    }
}
