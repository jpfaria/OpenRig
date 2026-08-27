//! Responsibility: holds every looper attached to one chain.
//! Issue #323 — the per-chain looper bank: the audio-thread slots plus the
//! lock-free channel the control thread uses to drive them.
//!
//! The split mirrors the block-toggle fast path (#580): the control thread
//! never takes the `processing` lock, it pushes a [`LooperOp`] onto an
//! `ArrayQueue`, and the audio thread drains the queue inside the section it
//! already owns. Layer buffers travel INSIDE the ops (allocated by the control
//! thread, moved into a slot by the audio thread) and travel back through
//! `retired` so the `Box` is dropped where dropping is allowed.
//!
//! Everything here belongs to ONE `ChainRuntimeState`. No queue, buffer or
//! atomic is shared with another chain — see the stream-isolation law.

use std::sync::atomic::Ordering;

use crate::looper::LooperSlot;
use crate::runtime_audio_frame::AudioFrame;
use block_core::AudioChannelLayout;

pub use crate::looper_op::LooperOp;
/// How many loopers one chain can hold — the domain rule lives in
/// `project::chain`, this is the runtime reading of it.
pub use project::chain::LOOPER_MAX_PER_CHAIN;
// `looper_bank_tests.rs` hangs off this module and builds ops through `super::`.
pub(crate) use crate::looper_status::state_code;
pub use crate::looper_status::LOOPER_MAX_SECONDS;
pub use crate::looper_status::{LooperShared, LooperStatus};
#[cfg(test)]
pub(crate) use project::chain::LooperSpeed;

/// One slot of the bank. `uid == 0` means the slot is free.
struct Entry {
    uid: u64,
    /// The chain segment this looper records from and plays on (the segment
    /// serving its chosen input endpoint).
    seg: usize,
    slot: LooperSlot,
}

/// The audio-thread side: the loopers themselves. Lives inside
/// `ChainProcessingState`, so the audio thread already holds `&mut` to it.
pub(crate) struct LooperBank {
    entries: Vec<Entry>,
    /// Per-frame scratch: the loop contribution accumulated across loopers.
    /// Sized once; never reallocated on the audio thread.
    active: usize,
}

impl LooperBank {
    pub(crate) fn new(max_frames: usize) -> Self {
        Self {
            entries: (0..LOOPER_MAX_PER_CHAIN)
                .map(|_| Entry {
                    uid: 0,
                    seg: 0,
                    slot: LooperSlot::new(max_frames),
                })
                .collect(),
            active: 0,
        }
    }

    /// Whether any slot is claimed — the audio thread skips the whole feature
    /// with one branch when no looper exists.
    pub(crate) fn is_idle(&self) -> bool {
        self.active == 0
    }

    /// Apply every queued op. Runs on the audio thread inside the existing
    /// `processing` lock; allocates nothing.
    pub(crate) fn drain_ops(&mut self, shared: &LooperShared) {
        while let Some(op) = shared.ops.pop() {
            self.apply(op, shared);
        }
    }

    fn apply(&mut self, op: LooperOp, shared: &LooperShared) {
        let uid = op.uid();
        if uid == 0 {
            self.give_back(op.take_buffer(), shared);
            return;
        }

        if let LooperOp::Create { uid, seg } = op {
            if let Some(existing) = self.index_of(uid) {
                // Re-created (e.g. input endpoint changed): keep the recorded
                // material, just move which segment it lives on.
                self.entries[existing].seg = seg;
            } else if let Some(free) = self.entries.iter_mut().find(|e| e.uid == 0) {
                free.uid = uid;
                free.seg = seg;
                free.slot.clear();
            }
            self.refresh_active();
            return;
        }

        let idx = match self.index_of(uid) {
            Some(i) => i,
            // An op for a looper this runtime does not hold: hand any buffer
            // back rather than dropping it here.
            None => {
                self.give_back(op.take_buffer(), shared);
                return;
            }
        };

        match op {
            LooperOp::Create { .. } => {}
            LooperOp::Remove { .. } => {
                self.entries[idx].slot.clear();
                self.entries[idx].uid = 0;
                self.refresh_active();
            }
            LooperOp::TapRecord { buffer, .. } => self.entries[idx].slot.tap_record(buffer),
            LooperOp::LoadLayer {
                buffer, len_frames, ..
            } => self.entries[idx].slot.load_layer(buffer, len_frames),
            LooperOp::Play { .. } => self.entries[idx].slot.play(),
            LooperOp::Stop { .. } => self.entries[idx].slot.stop(),
            LooperOp::Undo { .. } => self.entries[idx].slot.undo(),
            LooperOp::Redo { .. } => self.entries[idx].slot.redo(),
            LooperOp::Clear { .. } => self.entries[idx].slot.clear(),
            LooperOp::SetMix { value, .. } => self.entries[idx].slot.set_mix(value),
            LooperOp::SetDecay { value, .. } => self.entries[idx].slot.set_decay(value),
            LooperOp::SetSpeed { speed, .. } => self.entries[idx].slot.set_speed(speed),
            LooperOp::SetReverse { value, .. } => self.entries[idx].slot.set_reverse(value),
        }
    }

    /// Whether any looper lives on segment `seg` — lets the caller skip the
    /// per-frame work for a segment that has none.
    pub(crate) fn has_segment(&self, seg: usize) -> bool {
        self.entries.iter().any(|e| e.uid != 0 && e.seg == seg)
    }

    /// Capture segment `seg`'s dry input into the loopers that live on it.
    ///
    /// The bank is a RECORDER only: it writes the dry signal into the armed
    /// loopers' layers and leaves the chain frame untouched. Playback does NOT
    /// happen here — each looper plays on its own isolated stream routed to its
    /// chosen output (`arm_looper_stream`), independent of the record input.
    /// The `tick` return (the loop's own audio) is intentionally dropped: the
    /// loop is never summed back into the record chain.
    ///
    /// Called once per callback per segment (the caller drives every segment);
    /// a looper only records on its own segment, so a rig whose signal is on
    /// another input is captured, not silence.
    pub(crate) fn process(
        &mut self,
        seg: usize,
        frames: &mut [AudioFrame],
        _layout: AudioChannelLayout,
    ) {
        for frame in frames.iter() {
            let dry = match *frame {
                AudioFrame::Stereo(lr) => lr,
                AudioFrame::Mono(s) => [s, s],
            };
            for entry in self.entries.iter_mut() {
                if entry.uid == 0 || entry.seg != seg {
                    continue;
                }
                // Advances the slot's cursors and writes the dry frame into
                // the recording layer; the returned loop audio is discarded —
                // the isolated stream is what the listener hears.
                let _ = entry.slot.tick(dry);
            }
        }
    }

    /// Publish the slot state for the UI and hand retired buffers back.
    /// Runs at the end of the callback; wait-free.
    pub(crate) fn publish(&mut self, shared: &LooperShared) {
        for (cell, entry) in shared.status.iter().zip(self.entries.iter_mut()) {
            cell.uid.store(entry.uid, Ordering::Relaxed);
            cell.state
                .store(state_code(entry.slot.state()), Ordering::Relaxed);
            cell.position
                .store(entry.slot.position_frames(), Ordering::Relaxed);
            cell.len.store(entry.slot.len_frames(), Ordering::Relaxed);
            cell.layers
                .store(entry.slot.active_layers(), Ordering::Relaxed);
            cell.content_rev
                .store(entry.slot.content_revision(), Ordering::Relaxed);

            while let Some(buf) = entry.slot.take_retired() {
                push_retired(shared, buf);
            }
        }
    }

    /// Mixdown of one looper, for the control thread to save. Allocates —
    /// never called from the audio callback.
    pub(crate) fn export(&self, uid: u64) -> Option<Vec<f32>> {
        self.entries
            .iter()
            .find(|e| e.uid == uid)
            .and_then(|e| e.slot.export_mixdown())
    }

    fn index_of(&self, uid: u64) -> Option<usize> {
        self.entries.iter().position(|e| e.uid == uid)
    }

    fn refresh_active(&mut self) {
        self.active = self.entries.iter().filter(|e| e.uid != 0).count();
    }

    fn give_back(&self, buffer: Option<Box<[f32]>>, shared: &LooperShared) {
        if let Some(buf) = buffer {
            push_retired(shared, buf);
        }
    }
}

/// Park a buffer for the control thread to drop. If the return queue is full
/// — only reachable if the control thread stopped draining entirely — the
/// buffer is leaked on purpose: leaking is allowed on the audio thread,
/// freeing is not (invariant #8).
fn push_retired(shared: &LooperShared, buf: Box<[f32]>) {
    if let Err(buf) = shared.retired.push(buf) {
        std::mem::forget(buf);
    }
}

#[cfg(test)]
#[path = "looper_bank_tests.rs"]
mod tests;
