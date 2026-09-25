//! #979 — a route fed by a DSP worker must not shed the cushion it needs.
//!
//! The owner's rig: two guitars, one DSP worker per stream (#670), every
//! route on the Quantum HD 8 at 64 frames. The worker pushes each buffer a
//! few hundred microseconds after the input callback, so on most device
//! periods the output callback runs BEFORE that buffer lands, and on some it
//! runs after. The fill an output callback sees at its start therefore swings
//! by one whole callback buffer with the producer's phase — no latency is
//! gained or lost, it is just when the push landed.
//!
//! Live (2026-09-24, 0.5.1): after ~8 min of playing every route showed its
//! first `latency_trims`, then underruns climbed in bursts (0 → 2048 in three
//! minutes, trims 1 → 9) and never stopped until the chain was toggled.

use block_core::AudioChannelLayout;

use super::ElasticBuffer;
use crate::audio_frame::AudioFrame;
use crate::elastic_drift_guard::WINDOW_FRAMES;
use crate::runtime_audio_frame::elastic_target_for_buffer;

const FRAMES: usize = 64;
const CALLBACKS_PER_WINDOW: usize = WINDOW_FRAMES / FRAMES;

/// The route as production builds it: 64-frame device buffer, direct-callback
/// multiplier, convolver-fed on its producer's clock (cab IR) — primed with
/// its whole resting cushion (#965).
fn route() -> ElasticBuffer {
    let target = elastic_target_for_buffer(FRAMES as u32, 2);
    let buffer = ElasticBuffer::with_capacity(target, target * 2, AudioChannelLayout::Stereo);
    buffer.prime(target);
    buffer
}

fn push_buffer(route: &ElasticBuffer) {
    for _ in 0..FRAMES {
        route.push(AudioFrame::Stereo([0.25, 0.25]));
    }
}

fn output_callback(route: &ElasticBuffer) {
    let _fade = route.begin_callback(FRAMES);
    for _ in 0..FRAMES {
        route.pop();
    }
}

/// One device period. `worker_first`: the worker's push landed before the
/// output callback ran; otherwise it lands right after.
fn period(route: &ElasticBuffer, worker_first: bool) {
    if worker_first {
        push_buffer(route);
        output_callback(route);
    } else {
        output_callback(route);
        push_buffer(route);
    }
}

/// ~60 s of playing: the worker usually lands after the output callback, but
/// now and then a whole window of periods has it landing first. Nothing
/// stalls, no frame is lost or added, the producer keeps exact pace.
#[test]
fn a_worker_phase_swing_is_neither_trimmed_nor_underrun() {
    let route = route();
    for window in 0..320 {
        let worker_first = window % 16 == 15;
        for _ in 0..CALLBACKS_PER_WINDOW {
            period(&route, worker_first);
        }
    }
    assert_eq!(
        (route.latency_trims(), route.underrun_count()),
        (0, 0),
        "a producer that only changed WHEN its push landed was treated as stuck \
         latency: {} trims, {} underrun frames",
        route.latency_trims(),
        route.underrun_count()
    );
}

/// The worker falls four periods behind once per window (CPU contention: a
/// debug build, a compile in the background, a VST3 timer) and catches up
/// with the backlog; now and then a window goes by without a stall. No frame
/// is lost: every buffer the input delivered reaches the route. The first
/// stall underruns because the cushion was too shallow — that underrun
/// leaves exactly the missing cushion behind. From then on the route must
/// hold it instead of underrunning on every stall.
#[test]
fn a_route_keeps_the_cushion_an_underrun_proved_it_needs() {
    const DEPTH: usize = 4;
    const WARMUP_WINDOWS: usize = 40;
    let route = route();
    let mut after_warmup = None;
    for window in 0..400 {
        if window == WARMUP_WINDOWS {
            after_warmup = Some(route.underrun_count());
        }
        let stalls = window % 8 != 7;
        for callback in 0..CALLBACKS_PER_WINDOW {
            if stalls && callback == 40 {
                for _ in 0..DEPTH {
                    output_callback(&route);
                }
                for _ in 0..DEPTH {
                    push_buffer(&route);
                }
            }
            period(&route, false);
        }
    }
    let after_warmup = after_warmup.unwrap();
    assert_eq!(
        route.underrun_count(),
        after_warmup,
        "the route kept underrunning on every worker stall: {} underrun frames after \
         warm-up, {} trims — the cushion each underrun regrew was cut again",
        route.underrun_count() - after_warmup,
        route.latency_trims()
    );
}
