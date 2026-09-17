//! Responsibility: offers a chain's live input to the Tone Doctor.
//! #791 — split out of `tone_doctor_compact_wiring` (#948).

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use application::local_dispatcher::ToneDoctorCapture;
use application::tone_doctor_source::sum_streams;
use domain::ids::ChainId;

use crate::state::ProjectSession;

/// Record up to `seconds` of every guitar's mono input tap over the same
/// window, then sum them (#948: the owner's rule for a multi-guitar chain).
/// Polls the lock-free subscriptions; the player should be playing during the
/// window.
fn record(taps: Vec<Arc<dyn AudioTap>>, sr: f32, seconds: usize) -> Vec<[f32; 2]> {
    let target = seconds * sr as usize;
    let mut streams: Vec<Vec<f32>> = taps.iter().map(|_| Vec::with_capacity(target)).collect();
    let start = Instant::now();
    let deadline = Duration::from_secs(seconds as u64 + 2);
    while streams.iter().any(|mono| mono.len() < target) && start.elapsed() < deadline {
        let mut drained = 0;
        for (tap, mono) in taps.iter().zip(streams.iter_mut()) {
            let want = target - mono.len();
            if want > 0 {
                drained += tap.drain_channel(0, want, mono);
            }
        }
        if drained == 0 {
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    sum_streams(&streams)
}

/// Give the dispatcher a way to capture this machine's live input, so a chain
/// with no DI is diagnosable from any transport, not just from the GUI.
/// Subscribing happens on the calling thread; only the fill blocks — which is
/// why the subscription (not the authority that issues it) is what crosses
/// into the `Send` capture closure.
pub(crate) fn attach_live_input(session: &ProjectSession, taps: &Rc<dyn AudioTaps>) {
    let taps = Rc::clone(taps);
    session
        .dispatcher
        .attach_tone_doctor_input(Box::new(move |chain_id, seconds| {
            live_capture(taps.as_ref(), chain_id, seconds)
        }));
}

/// Prepare the capture of `chain_id`'s live input: subscribe now, fill later.
pub(crate) fn live_capture(
    taps: &dyn AudioTaps,
    chain_id: &ChainId,
    seconds: usize,
) -> Option<ToneDoctorCapture> {
    let sr = taps.live_sample_rate();
    let subscribed: Vec<Arc<dyn AudioTap>> = (0..taps.stream_count(chain_id))
        .filter_map(|stream| {
            taps.subscribe(
                &TapPoint::StreamInput {
                    chain: chain_id.clone(),
                    stream,
                },
                seconds * sr as usize,
            )
        })
        .collect();
    if subscribed.is_empty() {
        return None;
    }
    Some(Box::new(move || {
        Some((record(subscribed, sr as f32, seconds), sr as f32))
    }))
}

#[cfg(test)]
#[path = "tone_doctor_live_input_tests.rs"]
mod tests;
