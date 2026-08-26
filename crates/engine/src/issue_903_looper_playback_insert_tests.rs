//! #903 — a loop played on its isolated stream must be audible when the chain
//! holds an Insert.
//!
//! The looper (and the DI) play through `build_routed_di_runtime`, stepped by
//! the `di-stream` worker: the loop substitutes segment 0, the worker feeds
//! segment 0 and drains the chosen output route. An Insert splits the chain
//! into segments, so everything after it is a separate segment — and on the
//! owner's rig the loop reported `playing` with the cursor advancing while the
//! output stayed at the noise floor. These drive the same steps the worker
//! does and assert the loop reaches the tail.

use std::sync::Arc;

use super::tests::{insert_chain, insert_registry};
use crate::di_render::build_routed_di_runtime;
use crate::runtime::{process_input_f32, process_output_f32};
use crate::DiPcm;
use project::block::AudioBlockKind;
use project::chain::Chain;

const RATE: u32 = 48_000;
const BLOCK: usize = 256;

/// A steady non-silent stereo loop, one second long.
fn loop_pcm() -> DiPcm {
    DiPcm::new(vec![0.5_f32; RATE as usize * 2], RATE, 2)
}

/// Step the isolated runtime exactly the way `di_stream_worker` does and
/// return the peak the output route hands to the ring.
fn isolated_playback_peak(chain: &Chain) -> f32 {
    let routed = build_routed_di_runtime(chain, &insert_registry(), None, RATE, &loop_pcm())
        .expect("the isolated playback runtime must build");
    let silence = vec![0.0_f32; BLOCK];
    let mut drain = vec![0.0_f32; BLOCK * routed.drain_width];
    let mut peak = 0.0_f32;
    for i in 0..256 {
        process_input_f32(&routed.runtime, 0, &silence, 1);
        drain.fill(0.0);
        process_output_f32(
            &routed.runtime,
            routed.output_index,
            &mut drain,
            routed.drain_width,
        );
        // Ignore the fade-in ramp; measure once the playback is running.
        if i >= 128 {
            for frame in drain.chunks(routed.drain_width) {
                peak = peak
                    .max(frame[routed.drain_left].abs())
                    .max(frame[routed.drain_right].abs());
            }
        }
    }
    let _ = Arc::strong_count(&routed.runtime);
    peak
}

#[test]
fn a_loop_plays_through_a_chain_that_holds_an_insert() {
    let chain = insert_chain();

    let peak = isolated_playback_peak(&chain);

    assert!(
        peak > 0.01,
        "the loop must reach the output of a chain with an Insert — peak was {peak}"
    );
}

#[test]
fn a_loop_plays_through_the_same_chain_without_its_insert() {
    let mut chain = insert_chain();
    chain
        .blocks
        .retain(|b| !matches!(b.kind, AudioBlockKind::Insert(_)));

    let peak = isolated_playback_peak(&chain);

    assert!(
        peak > 0.01,
        "control: the same chain with no Insert plays the loop — peak was {peak}"
    );
}
