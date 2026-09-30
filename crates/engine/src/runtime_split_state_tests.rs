//! #328 spec §4.1 / §4.3: what a split preallocates at build, and how it
//! lines its paths up.

use domain::ids::BlockId;
use project::block::split_params::default_split_params;

use super::SplitRuntimeState;
use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::MAX_ALIGN_SAMPLES;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::test_support::delay_node;
use crate::runtime_state::{BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

fn state(a: Vec<BlockRuntimeNode>, b: Vec<BlockRuntimeNode>) -> SplitRuntimeState {
    SplitRuntimeState::new(
        true,
        a,
        b,
        SplitKnobs::from_params(&default_split_params()),
        &BlockId("split".into()),
    )
}

#[test]
fn the_shorter_path_is_delayed_by_the_difference() {
    let s = state(vec![delay_node("ir", 64)], vec![]);
    assert_eq!((s.align_a.delay(), s.align_b.delay()), (0, 64));
    let s = state(vec![], vec![delay_node("os", 7)]);
    assert_eq!((s.align_a.delay(), s.align_b.delay()), (7, 0));
}

#[test]
fn equal_paths_need_no_delay() {
    let s = state(vec![delay_node("ir_a", 64)], vec![delay_node("ir_b", 64)]);
    assert_eq!((s.align_a.delay(), s.align_b.delay()), (0, 0));
}

#[test]
fn a_switched_off_block_counts_again_after_realignment() {
    let mut ir = delay_node("ir", 64);
    ir.block_snapshot.enabled = false;
    let mut s = state(vec![ir], vec![]);
    assert_eq!(s.align_b.delay(), 0, "a bypassed block adds no delay");
    assert_eq!(
        s.align_b.capacity(),
        64,
        "room is kept for when it comes back"
    );
    s.a[0].block_snapshot.enabled = true;
    s.refresh_alignment();
    assert_eq!(s.align_b.delay(), 64);
}

#[test]
fn path_b_buffer_is_preallocated_for_the_largest_callback() {
    let s = state(vec![], vec![]);
    assert!(s.b_buf.capacity() >= SEGMENT_FRAME_CAPACITY);
}

#[test]
fn alignment_above_the_cap_is_clamped() {
    let s = state(
        vec![delay_node("lookahead", MAX_ALIGN_SAMPLES + 100)],
        vec![],
    );
    assert_eq!(s.align_b.delay(), MAX_ALIGN_SAMPLES);
}

#[test]
fn a_rebuilt_state_keeps_the_previous_delay_history() {
    let mut previous = state(vec![delay_node("ir", 4)], vec![]);
    let mut warm: Vec<AudioFrame> = (1..=10)
        .map(|i| AudioFrame::Stereo([i as f32, i as f32]))
        .collect();
    previous.align_b.process(&mut warm);
    let mut rebuilt = state(vec![delay_node("ir", 4)], vec![]);
    rebuilt.adopt_history(previous);
    let mut next = vec![
        AudioFrame::Stereo([11.0, 11.0]),
        AudioFrame::Stereo([12.0, 12.0]),
    ];
    rebuilt.align_b.process(&mut next);
    assert!(
        matches!(next[0], AudioFrame::Stereo([l, _]) if l == 7.0),
        "the delayed path continues from its history, got {:?}",
        next[0]
    );
}
