//! Responsibility: keeps the last seconds of one input stream as the HAL delivered it.
//!
//! The input callback records every buffer here before anything else touches
//! it; when the chain's input trips as stepped, a thread off the audio path
//! copies the ring into the restart's mark. The callback side is lock-free and
//! allocation-free: relaxed atomic stores into memory allocated (and written,
//! so its pages exist) when the stream was built.
//!
//! The reader never blocks the writer, so the oldest part of the ring may be
//! overwritten while it is copied. [`InputEvidenceRing::snapshot`] keeps only
//! the frames no callback could have reached during the copy: two of the
//! largest cycles seen behind the writer's position after the copy.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::input_evidence::{CycleRecord, InputEvidenceSnapshot, InputStreamIdentity};

/// Seconds of audio the ring holds: the detector trips after 1 s of stepped
/// windows and the health tick restarts within 2 s more.
const KEEP_SECONDS: usize = 3;

/// Callbacks whose timing is kept — 12 s at 64 frames / 44.1 kHz.
const CYCLE_SLOTS: usize = 8_192;

/// Cycles kept out of a snapshot at the overwritten end of the ring.
const TEAR_CYCLES: u64 = 2;

struct CycleSlot {
    host_ns: AtomicU64,
    first_frame: AtomicU64,
    frames: AtomicU32,
}

pub(crate) struct InputEvidenceRing {
    identity: InputStreamIdentity,
    channels: usize,
    capacity_frames: usize,
    /// `f32` bits, interleaved, `capacity_frames * channels` long.
    samples: Box<[AtomicU32]>,
    frames_written: AtomicU64,
    max_cycle_frames: AtomicU64,
    cycles: Box<[CycleSlot]>,
    cycles_written: AtomicU64,
}

impl InputEvidenceRing {
    /// A ring of [`KEEP_SECONDS`] at the stream's rate.
    pub(crate) fn new(identity: InputStreamIdentity) -> Self {
        let frames = identity.sample_rate as usize * KEEP_SECONDS;
        Self::with_capacity(identity, frames, CYCLE_SLOTS)
    }

    pub(crate) fn with_capacity(
        identity: InputStreamIdentity,
        capacity_frames: usize,
        cycle_slots: usize,
    ) -> Self {
        let channels = identity.channels.max(1);
        let capacity_frames = capacity_frames.max(1);
        Self {
            channels,
            capacity_frames,
            samples: (0..capacity_frames * channels)
                .map(|_| AtomicU32::new(0))
                .collect(),
            frames_written: AtomicU64::new(0),
            max_cycle_frames: AtomicU64::new(0),
            cycles: (0..cycle_slots.max(1))
                .map(|_| CycleSlot {
                    host_ns: AtomicU64::new(0),
                    first_frame: AtomicU64::new(0),
                    frames: AtomicU32::new(0),
                })
                .collect(),
            cycles_written: AtomicU64::new(0),
            identity,
        }
    }

    pub(crate) fn identity(&self) -> &InputStreamIdentity {
        &self.identity
    }

    #[cfg(test)]
    pub(crate) fn capacity_frames(&self) -> usize {
        self.capacity_frames
    }

    /// Audio thread: keep one callback's buffer. Only the stream's own
    /// callback calls this, so the positions have a single writer.
    pub(crate) fn record(&self, data: &[f32], host_ns: u64) {
        let frames = data.len() / self.channels;
        let first = self.frames_written.load(Ordering::Relaxed);
        self.max_cycle_frames
            .fetch_max(frames as u64, Ordering::Relaxed);
        for (k, frame) in data.chunks_exact(self.channels).enumerate() {
            let base = ((first + k as u64) % self.capacity_frames as u64) as usize * self.channels;
            for (c, sample) in frame.iter().enumerate() {
                self.samples[base + c].store(sample.to_bits(), Ordering::Relaxed);
            }
        }
        self.frames_written
            .store(first + frames as u64, Ordering::Release);

        let cycle = self.cycles_written.load(Ordering::Relaxed);
        let slot = &self.cycles[(cycle % self.cycles.len() as u64) as usize];
        slot.host_ns.store(host_ns, Ordering::Relaxed);
        slot.first_frame.store(first, Ordering::Relaxed);
        slot.frames.store(frames as u32, Ordering::Relaxed);
        self.cycles_written.store(cycle + 1, Ordering::Release);
    }

    /// Off the audio thread: copy what the ring holds that the writer could
    /// not have overwritten while it was copied.
    pub(crate) fn snapshot(&self) -> InputEvidenceSnapshot {
        let (first_frame, samples) = self.copy_samples();
        InputEvidenceSnapshot {
            first_frame,
            samples,
            cycles: self.copy_cycles(),
        }
    }

    fn copy_samples(&self) -> (u64, Vec<f32>) {
        let capacity = self.capacity_frames as u64;
        let end = self.frames_written.load(Ordering::Acquire);
        let copied: Vec<f32> = (end.saturating_sub(capacity)..end)
            .flat_map(|frame| {
                let base = (frame % capacity) as usize * self.channels;
                self.samples[base..base + self.channels]
                    .iter()
                    .map(|s| f32::from_bits(s.load(Ordering::Relaxed)))
            })
            .collect();
        let end_after = self.frames_written.load(Ordering::Acquire);
        let margin = TEAR_CYCLES * self.max_cycle_frames.load(Ordering::Relaxed);
        let start = end
            .saturating_sub(capacity)
            .max((end_after + margin).saturating_sub(capacity))
            .min(end);
        let skip = (start - end.saturating_sub(capacity)) as usize * self.channels;
        (start, copied[skip..].to_vec())
    }

    fn copy_cycles(&self) -> Vec<CycleRecord> {
        let slots = self.cycles.len() as u64;
        let end = self.cycles_written.load(Ordering::Acquire);
        let copied: Vec<CycleRecord> = (end.saturating_sub(slots)..end)
            .map(|cycle| {
                let slot = &self.cycles[(cycle % slots) as usize];
                CycleRecord {
                    host_ns: slot.host_ns.load(Ordering::Relaxed),
                    first_frame: slot.first_frame.load(Ordering::Relaxed),
                    frames: slot.frames.load(Ordering::Relaxed),
                }
            })
            .collect();
        let end_after = self.cycles_written.load(Ordering::Acquire);
        let start = end
            .saturating_sub(slots)
            .max((end_after + TEAR_CYCLES).saturating_sub(slots))
            .min(end);
        copied[(start - end.saturating_sub(slots)) as usize..].to_vec()
    }
}

#[cfg(test)]
#[path = "input_evidence_ring_tests.rs"]
mod tests;
