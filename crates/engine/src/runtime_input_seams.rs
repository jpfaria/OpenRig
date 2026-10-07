//! Responsibility: runs the #979 seam detector on the device channels each pipeline reads.
//!
//! Every pipeline watches its own input channels, so a broken input marks only
//! the runtime that reads it. A buffer lost to a busy processing lock is told to
//! the detectors as a discontinuity — our own skip must never read as a seam.

use std::sync::atomic::Ordering;

use crate::input_seam_detector::InputSeamDetector;
use crate::runtime_chain_state::ChainRuntimeState;
use crate::runtime_state::InputProcessingState;

/// One detector per device channel a pipeline reads.
pub(crate) struct InputSeamWatch {
    detectors: Vec<InputSeamDetector>,
    skips_seen: u64,
}

impl InputSeamWatch {
    pub(crate) fn new(channels: &[usize], sample_rate: f32) -> Self {
        Self {
            detectors: channels
                .iter()
                .map(|_| InputSeamDetector::new(sample_rate))
                .collect(),
            skips_seen: 0,
        }
    }

    fn is_stepped(&self) -> bool {
        self.detectors.iter().any(InputSeamDetector::is_tripped)
    }

    fn reset(&mut self) {
        for detector in &mut self.detectors {
            detector.reset();
        }
    }
}

/// Feeds one received device buffer to the pipelines it reaches, then marks the
/// runtime while any of its pipelines reads a stepped input — and unmarks it
/// once none does, so a restart never acts on a seam that is gone. Real-time
/// safe.
pub(crate) fn watch_input_seams(
    runtime: &ChainRuntimeState,
    input_states: &mut [InputProcessingState],
    segment_indices: &[usize],
    data: &[f32],
    input_total_channels: usize,
) {
    if runtime.seam_reset.load(Ordering::Relaxed)
        && runtime.seam_reset.swap(false, Ordering::AcqRel)
    {
        for state in input_states.iter_mut() {
            state.seam_watch.reset();
        }
    }
    let skips = runtime.input_busy_skips.load(Ordering::Relaxed);
    for &seg_idx in segment_indices {
        let Some(InputProcessingState {
            input_channels,
            seam_watch,
            ..
        }) = input_states.get_mut(seg_idx)
        else {
            continue;
        };
        let lost_buffer = seam_watch.skips_seen != skips;
        seam_watch.skips_seen = skips;
        for (detector, &channel) in seam_watch.detectors.iter_mut().zip(input_channels.iter()) {
            if lost_buffer {
                detector.discontinuity();
            }
            detector.push(data, input_total_channels, channel);
        }
    }
    let stepped = input_states
        .iter()
        .any(|state| state.seam_watch.is_stepped());
    runtime.input_stepped.store(stepped, Ordering::Relaxed);
}

impl ChainRuntimeState {
    /// #1081: the streams of this runtime were reopened, so its verdict starts
    /// over: unmarked now, and every detector empty on the next input buffer.
    /// Lock-free; called off the audio thread.
    pub fn reset_input_seams(&self) {
        self.seam_reset.store(true, Ordering::Release);
        self.input_stepped.store(false, Ordering::Relaxed);
    }
}
