use super::*;

use crate::resolved::ChainStreamSignature;
use crate::stream_handover::{STREAM_FADE_FRAMES, STREAM_WARMUP_FRAMES};

/// A stream set with no device streams behind it.
fn idle_set(swap: StreamSwap) -> ActiveChainRuntime {
    ActiveChainRuntime {
        stream_signature: ChainStreamSignature {
            inputs: vec![],
            outputs: vec![],
        },
        structure: Vec::new(),
        generation: 1,
        resolved: None,
        _input_streams: vec![],
        _output_streams: vec![],
        #[cfg(all(target_os = "linux", feature = "jack"))]
        _jack_client: None,
        #[cfg(all(target_os = "linux", feature = "jack"))]
        _dsp_worker: None,
        swap,
    }
}

/// Play `frames` frames through one output stream of `handover`'s set.
fn play(handover: &Arc<StreamHandover>, frames: usize) {
    let mut fade = handover.output_fade();
    let mut out = vec![1.0f32; 64 * 2];
    for _ in 0..frames / 64 {
        fade.apply(&mut out, 2);
    }
}

#[test]
fn a_replaced_set_keeps_playing_until_the_new_one_has_taken_over() {
    let mut new = StreamSwap::replacing();
    let at = Instant::now();
    new.retire(idle_set(StreamSwap::default()), at);

    assert_eq!(new.reap(at + Duration::from_millis(100)), 0);
    assert_eq!(new.retiring_len(), 1, "the old set still plays");

    play(&new.handover, STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES);
    assert_eq!(new.reap(at + Duration::from_millis(150)), 1);
    assert_eq!(new.retiring_len(), 0, "the new set plays on its own");
}

#[test]
fn a_replaced_set_goes_after_the_deadline_when_the_new_one_is_never_heard() {
    let mut new = StreamSwap::replacing();
    let at = Instant::now();
    new.retire(idle_set(StreamSwap::default()), at);

    assert_eq!(new.reap(at + RETIRE_DEADLINE - Duration::from_millis(1)), 0);
    assert_eq!(new.reap(at + RETIRE_DEADLINE), 1);
}

#[test]
fn a_replaced_set_fades_out_against_the_set_that_replaced_it() {
    let mut new = StreamSwap::replacing();
    let old = idle_set(StreamSwap::default());
    let mut old_fade = old.swap.handover.output_fade();
    new.retire(old, Instant::now());

    play(&new.handover, STREAM_WARMUP_FRAMES + STREAM_FADE_FRAMES);
    let mut out = vec![1.0f32; 64 * 2];
    old_fade.apply(&mut out, 2);
    assert!(
        out.iter().all(|&g| g == 0.0),
        "the old set is silent once the new one has taken over"
    );
}

#[test]
fn a_set_that_replaces_nothing_has_nothing_to_retire() {
    let mut cold = StreamSwap::default();
    assert_eq!(cold.retiring_len(), 0);
    assert_eq!(cold.reap(Instant::now()), 0);
    assert!(cold.handover.has_taken_over());
}
