//! Responsibility: places every take of the project on one shared loop timeline.
//!
//! The first take opened over silence sets the anchor (the top of the cycle)
//! and the cycle length. Every later take, on any chain, closes on a whole
//! number of cycles of the shortest loop already recorded and is stored
//! rotated so its frame 0 is the top of the cycle. Lengths come from the time
//! between the two presses on the host clock, not from when the meter tick
//! happened to drain the samples, so a late drain never shortens a take.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use domain::ids::ChainId;
use engine::LooperState;

use super::{drain_retired, LoopEntry, LooperStore};
use crate::loop_sync::{frames_between, phase_frames, quantize_take_len, PLAY_LEAD_NS};

/// The clock the store reads presses from, in host nanoseconds.
pub(crate) type LoopClock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// A take being recorded.
pub(super) struct Take {
    /// When REC opened it.
    open_ns: u64,
    /// When its first sample was captured, set by the audio thread through
    /// the input tap. `None` when the tap carries no capture time: the take
    /// is then exactly what was captured.
    first_capture: Option<Arc<AtomicU64>>,
    /// Set once the take was asked to close.
    close: Option<Close>,
}

struct Close {
    /// The take's final length, in frames.
    target: usize,
    /// Land in Stopped instead of Playing.
    then_stop: bool,
}

impl LooperStore {
    /// Replace the clock presses are read from.
    pub fn set_clock(&mut self, clock: LoopClock) {
        self.clock = clock;
    }

    /// The top of the shared cycle, in host nanoseconds, once a take set it.
    pub fn sync_anchor(&self) -> Option<u64> {
        self.anchor_ns
    }

    /// Hand the take being recorded the cell its input tap stamps with the
    /// capture time of its first sample.
    pub fn set_recording_stamp(&mut self, chain: &ChainId, uid: u64, stamp: Arc<AtomicU64>) {
        if let Some(take) = self
            .slots
            .get_mut(&(chain.clone(), uid))
            .and_then(|e| e.take.as_mut())
        {
            take.first_capture = Some(stamp);
        }
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Whether any looper other than `(chain, uid)` is playing or recording.
    fn sounding_except(&self, chain: &ChainId, uid: u64) -> bool {
        self.slots.iter().any(|((c, u), e)| {
            !(c == chain && *u == uid)
                && matches!(
                    e.slot.state(),
                    LooperState::Playing | LooperState::Overdubbing | LooperState::Recording
                )
        })
    }

    /// The shortest recorded loop other than `(chain, uid)`; 0 when none.
    fn cycle_except(&self, chain: &ChainId, uid: u64) -> usize {
        self.slots
            .iter()
            .filter(|((c, u), e)| {
                !(c == chain && *u == uid)
                    && matches!(e.slot.state(), LooperState::Playing | LooperState::Stopped)
                    && e.slot.len_frames() > 0
            })
            .map(|(_, e)| e.slot.len_frames())
            .min()
            .unwrap_or(0)
    }

    /// REC opened a take on `(chain, uid)`. Over silence it restarts the
    /// timeline at this press.
    pub(super) fn open_take(&mut self, chain: &ChainId, uid: u64) {
        let now = self.now();
        if self.anchor_ns.is_none() || !self.sounding_except(chain, uid) {
            self.anchor_ns = Some(now);
        }
        if let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) {
            entry.take = Some(Take {
                open_ns: now,
                first_capture: None,
                close: None,
            });
        }
    }

    /// Whether `(chain, uid)` is recording a take that was not closed yet.
    pub(super) fn has_open_take(&self, chain: &ChainId, uid: u64) -> bool {
        self.slots
            .get(&(chain.clone(), uid))
            .is_some_and(|e| e.slot.state() == LooperState::Recording && e.take.is_some())
    }

    /// Ask the take on `(chain, uid)` to close now: its length is the time
    /// since REC rounded to whole cycles. It keeps recording until that many
    /// frames have arrived.
    pub(super) fn begin_close(&mut self, chain: &ChainId, uid: u64, then_stop: bool) {
        let now = self.now();
        let rate = self.sample_rate;
        let cycle = self.cycle_except(chain, uid);
        let max = self.max_frames();
        let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) else {
            return;
        };
        let captured = entry.slot.len_frames();
        let Some(take) = entry.take.as_mut() else {
            return;
        };
        if let Some(close) = take.close.as_mut() {
            close.then_stop = then_stop;
            return;
        }
        let raw = if take.first_capture.is_some() {
            frames_between(take.open_ns, now, rate) as usize
        } else {
            captured
        };
        take.close = Some(Close {
            target: quantize_take_len(raw, cycle).clamp(1, max),
            then_stop,
        });
        self.finish_take_if_complete(chain, uid);
    }

    /// Feed interleaved-stereo `frames` to `entry`, never past the length
    /// its closing take is waiting for.
    pub(super) fn feed(entry: &mut LoopEntry, frames: &[f32]) {
        let target = entry
            .take
            .as_ref()
            .and_then(|t| t.close.as_ref())
            .map(|c| c.target);
        for f in frames.chunks_exact(2) {
            if target.is_some_and(|t| {
                entry.slot.state() == LooperState::Recording && entry.slot.len_frames() >= t
            }) {
                break;
            }
            let _ = entry.slot.tick([f[0], f[1]]);
        }
    }

    /// Close the take on `(chain, uid)` once it holds its length: freeze it
    /// at that length and rotate it onto the shared timeline.
    pub(super) fn finish_take_if_complete(&mut self, chain: &ChainId, uid: u64) {
        let anchor = self.anchor_ns;
        let rate = self.sample_rate;
        let Some(entry) = self.slots.get_mut(&(chain.clone(), uid)) else {
            return;
        };
        let Some(take) = entry.take.as_ref() else {
            return;
        };
        let recording = entry.slot.state() == LooperState::Recording;
        let Some(close) = take.close.as_ref() else {
            // The slot froze itself at its longest length.
            if !recording {
                entry.take = None;
                entry.rings.clear();
            }
            return;
        };
        let stamped = take.first_capture.is_some();
        if recording && stamped && entry.slot.len_frames() < close.target {
            return;
        }
        let (target, then_stop) = (close.target, close.then_stop);
        let first_capture = take
            .first_capture
            .as_ref()
            .map(|s| s.load(Ordering::Relaxed))
            .filter(|&ns| ns != 0)
            .unwrap_or(take.open_ns);
        if recording {
            entry.slot.tap_record(None);
            drain_retired(&mut entry.slot);
        }
        entry.slot.set_len_frames(target);
        if let (true, Some(anchor)) = (stamped, anchor) {
            entry
                .slot
                .rotate_right(phase_frames(anchor, first_capture, rate, target));
        }
        entry.take = None;
        entry.rings.clear();
        if then_stop {
            entry.slot.stop();
        }
    }

    /// Before a loop starts over silence, restart the timeline one lead ahead
    /// so every loop started now begins at the top together.
    pub(super) fn reanchor_if_silent(&mut self, chain: &ChainId, uid: u64) {
        if !self.sounding_except(chain, uid) {
            self.anchor_ns = Some(self.now() + PLAY_LEAD_NS);
        }
    }
}
