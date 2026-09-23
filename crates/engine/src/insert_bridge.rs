//! Responsibility: carries a bypassed insert's dry signal from its send segment to its return segment.
//!
//! Issue #967. An insert cuts the chain in two segments driven by two device
//! callbacks: the send segment writes the external gear, the return segment
//! reads what comes back. Until now, switching the insert OFF removed the cut
//! altogether — which changed how many streams the chain needed, so a one-bit
//! flip closed and reopened every stream the chain owned (measured 2.1 s and
//! 3.0 s of silence on the owner's rig).
//!
//! The streams now stay exactly as they are and the loop is bypassed HERE: the
//! send segment keeps feeding the gear and also parks its dry frames in this
//! bridge; the return segment reads them instead of the return endpoint. The
//! cost is one callback of delay on the bypassed path — the two segments are
//! driven by different callbacks, which is true whether the detour is a cable
//! or this buffer.

use std::collections::VecDeque;

use crate::audio_frame::AudioFrame;

/// The dry path between one insert's two segments.
pub(crate) struct InsertBridge {
    /// Is the insert switched off (the loop bypassed)?
    pub(crate) bypassed: bool,
    /// Dry frames the send segment parked for the return segment. Bounded and
    /// pre-allocated: pushing past the capacity drops the OLDEST frame, so a
    /// return callback that stops pulling can never grow this.
    frames: VecDeque<AudioFrame>,
}

impl InsertBridge {
    /// `capacity` frames of slack — a few callbacks' worth, so the two streams
    /// may drift a little without the bridge running dry or growing.
    pub(crate) fn new(bypassed: bool, capacity: usize) -> Self {
        Self {
            bypassed,
            frames: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Park this callback's dry frames. Allocation-free: the deque never grows
    /// past the capacity it was built with.
    pub(crate) fn park(&mut self, frames: &[AudioFrame]) {
        let capacity = self.frames.capacity();
        for &frame in frames {
            if self.frames.len() == capacity {
                self.frames.pop_front();
            }
            self.frames.push_back(frame);
        }
    }

    /// Take `count` dry frames into `out`. Short reads pad with silence — the
    /// return callback can fire before the send callback has produced anything
    /// (right after the bypass is switched on, or when the two devices drift).
    pub(crate) fn take(&mut self, count: usize, silence: AudioFrame, out: &mut Vec<AudioFrame>) {
        for _ in 0..count {
            out.push(self.frames.pop_front().unwrap_or(silence));
        }
    }

    /// Drop whatever is parked — used when the bypass is switched off so the
    /// next bypass starts from live frames instead of stale ones.
    pub(crate) fn clear(&mut self) {
        self.frames.clear();
    }
}

#[cfg(test)]
#[path = "insert_bridge_tests.rs"]
mod tests;
