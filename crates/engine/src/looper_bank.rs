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

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};

use crossbeam_queue::ArrayQueue;
use project::chain::LooperSpeed;

use crate::looper::{LooperSlot, LooperState};
use crate::runtime_audio_frame::AudioFrame;
use block_core::AudioChannelLayout;

/// How many loopers one chain can hold — the domain rule lives in
/// `project::chain`, this is the runtime reading of it.
pub use project::chain::LOOPER_MAX_PER_CHAIN;
/// Longest loop a single looper can record.
pub const LOOPER_MAX_SECONDS: f32 = 60.0;
/// Depth of the control → audio op queue. Far above any plausible burst of
/// footswitch taps; a full queue means the audio thread has stalled.
const OP_QUEUE_DEPTH: usize = 64;
/// Depth of the audio → control buffer-return queue. One entry per layer of
/// every looper, so a full `clear` of every slot always fits.
const RETIRE_QUEUE_DEPTH: usize = LOOPER_MAX_PER_CHAIN * crate::looper::LOOPER_MAX_LAYERS;

/// A control-thread request for one looper. Ops carrying a buffer hand its
/// ownership to the audio thread.
#[derive(Debug)]
pub enum LooperOp {
    /// Claim a free slot for `uid`.
    Create {
        uid: u64,
    },
    /// Free the slot and return every layer it holds.
    Remove {
        uid: u64,
    },
    /// The record/overdub footswitch tap. `buffer` must be `Some` when the tap
    /// starts a recording (see [`LooperSlot::tap_record`]).
    TapRecord {
        uid: u64,
        buffer: Option<Box<[f32]>>,
    },
    /// Install a layer recorded earlier (restored from disk) as the base
    /// layer of an empty looper, `len_frames` long.
    LoadLayer {
        uid: u64,
        buffer: Box<[f32]>,
        len_frames: usize,
    },
    Play {
        uid: u64,
    },
    Stop {
        uid: u64,
    },
    Undo {
        uid: u64,
    },
    Redo {
        uid: u64,
    },
    Clear {
        uid: u64,
    },
    SetMix {
        uid: u64,
        value: f32,
    },
    SetDecay {
        uid: u64,
        value: f32,
    },
    SetSpeed {
        uid: u64,
        speed: LooperSpeed,
    },
    SetReverse {
        uid: u64,
        value: bool,
    },
}

impl LooperOp {
    fn uid(&self) -> u64 {
        match self {
            Self::Create { uid }
            | Self::Remove { uid }
            | Self::TapRecord { uid, .. }
            | Self::LoadLayer { uid, .. }
            | Self::Play { uid }
            | Self::Stop { uid }
            | Self::Undo { uid }
            | Self::Redo { uid }
            | Self::Clear { uid }
            | Self::SetMix { uid, .. }
            | Self::SetDecay { uid, .. }
            | Self::SetSpeed { uid, .. }
            | Self::SetReverse { uid, .. } => *uid,
        }
    }

    /// Take the buffer out of an op that carries one.
    fn take_buffer(self) -> Option<Box<[f32]>> {
        match self {
            Self::TapRecord { buffer, .. } => buffer,
            Self::LoadLayer { buffer, .. } => Some(buffer),
            _ => None,
        }
    }
}

/// What the GUI / MCP / gRPC read about one looper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LooperStatus {
    pub uid: u64,
    pub state: LooperState,
    pub position_frames: usize,
    pub len_frames: usize,
    pub layers: usize,
}

/// Lock-free mirror of one slot, written by the audio thread once per
/// callback and read by any thread without locking.
#[derive(Default)]
struct StatusCell {
    uid: AtomicU64,
    state: AtomicU8,
    position: AtomicUsize,
    len: AtomicUsize,
    layers: AtomicUsize,
}

fn state_code(state: LooperState) -> u8 {
    match state {
        LooperState::Empty => 0,
        LooperState::Recording => 1,
        LooperState::Playing => 2,
        LooperState::Overdubbing => 3,
        LooperState::Stopped => 4,
    }
}

fn state_from_code(code: u8) -> LooperState {
    match code {
        1 => LooperState::Recording,
        2 => LooperState::Playing,
        3 => LooperState::Overdubbing,
        4 => LooperState::Stopped,
        _ => LooperState::Empty,
    }
}

/// The control ↔ audio channel for one chain's loopers.
pub struct LooperShared {
    ops: ArrayQueue<LooperOp>,
    retired: ArrayQueue<Box<[f32]>>,
    status: Vec<StatusCell>,
    /// Longest loop, in frames, at this runtime's live sample rate. The
    /// control thread reads it to size the buffers it allocates.
    max_frames: usize,
}

impl LooperShared {
    pub(crate) fn new(sample_rate: f32) -> Self {
        Self {
            ops: ArrayQueue::new(OP_QUEUE_DEPTH),
            retired: ArrayQueue::new(RETIRE_QUEUE_DEPTH),
            status: (0..LOOPER_MAX_PER_CHAIN)
                .map(|_| StatusCell::default())
                .collect(),
            max_frames: (LOOPER_MAX_SECONDS * sample_rate.max(1.0)) as usize,
        }
    }

    pub(crate) fn max_frames(&self) -> usize {
        self.max_frames
    }

    /// Queue an op for the audio thread. `Err` returns the op when the queue
    /// is full — i.e. the audio thread is not draining.
    pub(crate) fn push(&self, op: LooperOp) -> Result<(), LooperOp> {
        self.ops.push(op)
    }

    /// Collect the layer buffers the audio thread is done with, so they are
    /// dropped off the audio thread.
    pub(crate) fn drain_retired(&self) -> Vec<Box<[f32]>> {
        let mut out = Vec::new();
        while let Some(buf) = self.retired.pop() {
            out.push(buf);
        }
        out
    }

    pub(crate) fn status(&self, uid: u64) -> Option<LooperStatus> {
        self.status
            .iter()
            .find(|c| c.uid.load(Ordering::Relaxed) == uid && uid != 0)
            .map(|c| LooperStatus {
                uid,
                state: state_from_code(c.state.load(Ordering::Relaxed)),
                position_frames: c.position.load(Ordering::Relaxed),
                len_frames: c.len.load(Ordering::Relaxed),
                layers: c.layers.load(Ordering::Relaxed),
            })
    }

    /// Every live looper of this chain, in slot order.
    pub(crate) fn statuses(&self) -> Vec<LooperStatus> {
        self.status
            .iter()
            .filter_map(|c| {
                let uid = c.uid.load(Ordering::Relaxed);
                (uid != 0).then(|| LooperStatus {
                    uid,
                    state: state_from_code(c.state.load(Ordering::Relaxed)),
                    position_frames: c.position.load(Ordering::Relaxed),
                    len_frames: c.len.load(Ordering::Relaxed),
                    layers: c.layers.load(Ordering::Relaxed),
                })
            })
            .collect()
    }

    /// Copy the published status of a superseded runtime (chain rebuild), so
    /// the UI does not blink through "no loopers" mid-swap.
    pub(crate) fn adopt_status_from(&self, other: &Self) {
        for (dst, src) in self.status.iter().zip(other.status.iter()) {
            dst.uid
                .store(src.uid.load(Ordering::Relaxed), Ordering::Relaxed);
            dst.state
                .store(src.state.load(Ordering::Relaxed), Ordering::Relaxed);
            dst.position
                .store(src.position.load(Ordering::Relaxed), Ordering::Relaxed);
            dst.len
                .store(src.len.load(Ordering::Relaxed), Ordering::Relaxed);
            dst.layers
                .store(src.layers.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }
}

/// One slot of the bank. `uid == 0` means the slot is free.
struct Entry {
    uid: u64,
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

        if let LooperOp::Create { uid } = op {
            if self.index_of(uid).is_none() {
                if let Some(free) = self.entries.iter_mut().find(|e| e.uid == 0) {
                    free.uid = uid;
                    free.slot.clear();
                }
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

    /// Record the chain input into every armed looper and sum the playing
    /// loops back into it. Called once per callback, on the chain's first
    /// segment only (#699: a chain's loop material plays exactly once).
    pub(crate) fn process(&mut self, frames: &mut [AudioFrame], layout: AudioChannelLayout) {
        for frame in frames.iter_mut() {
            let dry = match *frame {
                AudioFrame::Stereo(lr) => lr,
                AudioFrame::Mono(s) => [s, s],
            };
            let mut loop_sum = [0.0f32; 2];
            for entry in self.entries.iter_mut() {
                if entry.uid == 0 {
                    continue;
                }
                // Every looper records the SAME dry input — a loop never
                // feeds another loop (no wet feedback path).
                let contribution = entry.slot.tick(dry);
                loop_sum[0] += contribution[0];
                loop_sum[1] += contribution[1];
            }
            *frame = match layout {
                AudioChannelLayout::Stereo => {
                    AudioFrame::Stereo([dry[0] + loop_sum[0], dry[1] + loop_sum[1]])
                }
                AudioChannelLayout::Mono => AudioFrame::Mono(
                    dry[0].mul_add(0.5, dry[1] * 0.5) + (loop_sum[0] + loop_sum[1]) * 0.5,
                ),
            };
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

            while let Some(buf) = entry.slot.take_retired() {
                push_retired(shared, buf);
            }
        }
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
