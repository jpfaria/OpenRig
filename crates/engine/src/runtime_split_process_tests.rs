//! #328 spec §4.1 steps 1–4: fill path B's buffer, run both paths, delay
//! the shorter one, mix back into the bus.

use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::split_params::{self, default_split_params};

use super::process_split;
use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_process_segment::process_audio_block;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::{delay_node, gain_node};
use crate::runtime_state::{BlockError, BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

fn knobs(overrides: &[(&str, ParameterValue)]) -> SplitKnobs {
    let mut params = default_split_params();
    for (key, value) in overrides {
        params.insert(*key, value.clone());
    }
    SplitKnobs::from_params(&params)
}

fn split(
    mixes: bool,
    a: Vec<BlockRuntimeNode>,
    b: Vec<BlockRuntimeNode>,
    overrides: &[(&str, ParameterValue)],
) -> SplitRuntimeState {
    SplitRuntimeState::new(mixes, a, b, knobs(overrides), &BlockId("split".into()))
}

fn run(state: &mut SplitRuntimeState, frames: &mut [AudioFrame]) {
    let queue = ArrayQueue::<BlockError>::new(8);
    process_split(state, frames, |node, path| {
        process_audio_block(node, path, &queue)
    });
}

fn sine(n: usize, start: usize) -> Vec<AudioFrame> {
    (start..start + n)
        .map(|i| {
            let s = 0.5 * (i as f32 * 0.05).sin();
            AudioFrame::Stereo([s, s])
        })
        .collect()
}

fn pair(frame: AudioFrame) -> [f32; 2] {
    match frame {
        AudioFrame::Stereo(pair) => pair,
        AudioFrame::Mono(s) => [s, s],
    }
}

fn assert_half_of(out: &[AudioFrame], input: &[AudioFrame]) {
    for (i, (o, x)) in out.iter().zip(input).enumerate() {
        let (o, x) = (pair(*o), pair(*x));
        assert!(
            (o[0] - 0.5 * x[0]).abs() < 1e-6 && (o[1] - 0.5 * x[1]).abs() < 1e-6,
            "frame {i}: {o:?} is not half of {x:?}"
        );
    }
}

#[test]
fn identical_paths_at_default_knobs_come_out_at_the_path_level() {
    let mut s = split(
        true,
        vec![gain_node("a", 0.5)],
        vec![gain_node("b", 0.5)],
        &[],
    );
    let input = sine(256, 0);
    let mut frames = input.clone();
    run(&mut s, &mut frames);
    assert_half_of(&frames, &input);
}

#[test]
fn inverted_b_cancels_an_identical_but_later_path_a() {
    let mut s = split(
        true,
        vec![delay_node("ir", 64)],
        vec![],
        &[(
            split_params::MIX_B_POLARITY,
            ParameterValue::String(split_params::POLARITY_INVERT.into()),
        )],
    );
    for callback in 0..3 {
        let mut frames = sine(128, callback * 128);
        run(&mut s, &mut frames);
        let peak = frames
            .iter()
            .map(|f| pair(*f)[0].abs())
            .fold(0.0_f32, f32::max);
        assert!(
            peak < 1e-6,
            "callback {callback}: aligned paths must cancel, peak {peak}"
        );
    }
}

#[test]
fn y_end_sums_both_paths_at_unity_ignoring_the_mixer() {
    let mut s = split(
        false,
        vec![],
        vec![],
        &[
            (split_params::MIX_PAN_A, ParameterValue::Float(-50.0)),
            (split_params::MIX_MASTER, ParameterValue::Float(10.0)),
        ],
    );
    let mut frames = vec![AudioFrame::Stereo([0.2, 0.1]); 4];
    run(&mut s, &mut frames);
    for frame in frames {
        let out = pair(frame);
        assert!(
            (out[0] - 0.4).abs() < 1e-6 && (out[1] - 0.2).abs() < 1e-6,
            "{out:?}"
        );
    }
}

#[test]
fn a_mono_bus_comes_out_stereo() {
    let mut s = split(true, vec![], vec![], &[]);
    let mut frames = vec![AudioFrame::Mono(0.3); 4];
    run(&mut s, &mut frames);
    assert!(frames.iter().all(|f| matches!(
        f,
        AudioFrame::Stereo([l, r]) if (*l - 0.3).abs() < 1e-6 && (*r - 0.3).abs() < 1e-6
    )));
}

#[test]
fn a_callback_larger_than_the_preallocated_buffer_still_mixes_every_frame() {
    let mut s = split(
        true,
        vec![gain_node("a", 0.5)],
        vec![gain_node("b", 0.5)],
        &[],
    );
    let input = sine(2 * SEGMENT_FRAME_CAPACITY, 0);
    let mut frames = input.clone();
    run(&mut s, &mut frames);
    assert_half_of(&frames, &input);
}
