//! #127: the contract of the subscription seam, independent of any frontend.
//!
//! The point of the defaults is that a transport which hosts NO audio still
//! compiles, still answers, and degrades to silence instead of lying — and that
//! raw PCM is an in-process affordance nobody is required to provide.

use super::*;

/// A tap that only implements the reduced reading — the shape a remote
/// frontend can honour over a wire.
struct ReducedOnlyTap(f32);

impl AudioTap for ReducedOnlyTap {
    fn channels(&self) -> usize {
        1
    }
    fn poll_peak_dbfs(&self) -> f32 {
        self.0
    }
}

#[test]
fn a_tap_that_carries_no_pcm_still_answers_the_reduced_reading() {
    // The whole PCM decision in one assertion: an implementation that cannot
    // hand out samples (anything not in this process) is a COMPLETE
    // implementation of the seam. It reports the number the meters need and
    // gives no window, rather than being unimplementable.
    let tap = ReducedOnlyTap(-12.5);
    assert_eq!(tap.poll_peak_dbfs(), -12.5);

    let mut out = Vec::new();
    assert_eq!(
        tap.drain_channel(0, 1024, &mut out),
        0,
        "an out-of-process tap has no samples to give"
    );
    assert!(
        out.is_empty(),
        "and must not fabricate any — a consumer that needs PCM shows nothing"
    );
}

#[test]
fn a_frontend_that_hosts_no_audio_subscribes_to_nothing() {
    let taps = NoAudioTaps;
    assert!(!taps.is_hosted());
    assert_eq!(taps.live_sample_rate(), 0, "never a fabricated rate (#723)");
    assert_eq!(taps.stream_count(&ChainId("chain:0".into())), 0);
    assert!(
        taps.subscribe(
            &TapPoint::StreamInput {
                chain: ChainId("chain:0".into()),
                stream: 0,
            },
            256,
        )
        .is_none(),
        "no runtime ⇒ no tap; never a handle onto a different stream"
    );
}

#[test]
fn a_tap_point_names_one_stream_of_one_chain() {
    // `CLAUDE.md` LAW, pinned as a type property: the same stream index on two
    // different chains is two different tap points, and nothing in the enum
    // can express "every runtime that matches".
    let a = TapPoint::StreamOutput {
        chain: ChainId("chain:a".into()),
        stream: 0,
    };
    let b = TapPoint::StreamOutput {
        chain: ChainId("chain:b".into()),
        stream: 0,
    };
    assert_ne!(a, b, "identity is (chain, stream), never the index alone");

    let other_stream = TapPoint::StreamOutput {
        chain: ChainId("chain:a".into()),
        stream: 1,
    };
    assert_ne!(a, other_stream);
}
