//! #328 spec §4.1 / §4.3 / §11.4: what a split preallocates at build, and how
//! it lines its paths up — every path against the longest.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;

use super::SplitRuntimeState;
use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::MAX_ALIGN_SAMPLES;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::test_support::delay_node;
use crate::runtime_state::{BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

fn state(paths: Vec<Vec<BlockRuntimeNode>>) -> SplitRuntimeState {
    let count = paths.len();
    SplitRuntimeState::new(
        true,
        paths,
        SplitKnobs::from_params(&default_split_params(count), count),
        &BlockId("split".into()),
    )
}

fn delays(s: &SplitRuntimeState) -> Vec<usize> {
    s.aligns.iter().map(|a| a.delay()).collect()
}

#[test]
fn the_shorter_path_is_delayed_by_the_difference() {
    let s = state(vec![vec![delay_node("ir", 64)], vec![]]);
    assert_eq!(delays(&s), vec![0, 64]);
    let s = state(vec![vec![], vec![delay_node("os", 7)]]);
    assert_eq!(delays(&s), vec![7, 0]);
}

#[test]
fn every_path_is_delayed_against_the_longest() {
    let s = state(vec![
        vec![delay_node("os", 7)],
        vec![delay_node("ir", 64)],
        vec![],
        vec![delay_node("long", 71)],
    ]);
    assert_eq!(delays(&s), vec![64, 7, 71, 0]);
}

#[test]
fn equal_paths_need_no_delay() {
    let s = state(vec![
        vec![delay_node("ir_a", 64)],
        vec![delay_node("ir_b", 64)],
    ]);
    assert_eq!(delays(&s), vec![0, 0]);
}

#[test]
fn a_switched_off_block_counts_again_after_realignment() {
    let mut ir = delay_node("ir", 64);
    ir.block_snapshot.enabled = false;
    let mut s = state(vec![vec![ir], vec![]]);
    assert_eq!(s.aligns[1].delay(), 0, "a bypassed block adds no delay");
    assert_eq!(
        s.aligns[1].capacity(),
        64,
        "room is kept for when it comes back"
    );
    s.paths[0][0].block_snapshot.enabled = true;
    s.refresh_alignment();
    assert_eq!(s.aligns[1].delay(), 64);
}

#[test]
fn every_path_buffer_is_preallocated_for_the_largest_callback() {
    let s = state(vec![vec![], vec![], vec![]]);
    assert_eq!(s.bufs.len(), 3);
    assert_eq!(s.values.len(), 3);
    assert!(s
        .bufs
        .iter()
        .all(|b| b.capacity() >= SEGMENT_FRAME_CAPACITY));
}

#[test]
fn alignment_above_the_cap_is_clamped() {
    let s = state(vec![
        vec![delay_node("lookahead", MAX_ALIGN_SAMPLES + 100)],
        vec![],
    ]);
    assert_eq!(s.aligns[1].delay(), MAX_ALIGN_SAMPLES);
}

#[test]
fn a_rebuilt_state_keeps_the_previous_delay_history() {
    let mut previous = state(vec![vec![delay_node("ir", 4)], vec![]]);
    let mut warm: Vec<AudioFrame> = (1..=10)
        .map(|i| AudioFrame::Stereo([i as f32, i as f32]))
        .collect();
    previous.aligns[1].process(&mut warm);
    let mut rebuilt = state(vec![vec![delay_node("ir", 4)], vec![]]);
    rebuilt.adopt_history(previous);
    let mut next = vec![
        AudioFrame::Stereo([11.0, 11.0]),
        AudioFrame::Stereo([12.0, 12.0]),
    ];
    rebuilt.aligns[1].process(&mut next);
    assert!(
        matches!(next[0], AudioFrame::Stereo([l, _]) if l == 7.0),
        "the delayed path continues from its history, got {:?}",
        next[0]
    );
}
