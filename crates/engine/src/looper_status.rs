//! Responsibility: publishes a looper's state to whoever is watching.

use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};

use crossbeam_queue::ArrayQueue;

use crate::looper::LooperState;
use crate::looper_op::LooperOp;

pub use project::chain::LOOPER_MAX_PER_CHAIN;
pub const LOOPER_MAX_SECONDS: f32 = 60.0;
const OP_QUEUE_DEPTH: usize = 64;
const RETIRE_QUEUE_DEPTH: usize = LOOPER_MAX_PER_CHAIN * crate::looper::LOOPER_MAX_LAYERS;

/// What the GUI / MCP / gRPC read about one looper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LooperStatus {
    pub uid: u64,
    pub state: LooperState,
    pub position_frames: usize,
    pub len_frames: usize,
    pub layers: usize,
    /// #323: changes whenever the exported mixdown would change (record close,
    /// overdub, undo/redo, clear, level/decay/reverse) — the controller re-arms
    /// the isolated stream only when this moves.
    pub content_rev: u64,
}

/// Lock-free mirror of one slot, written by the audio thread once per
/// callback and read by any thread without locking.
#[derive(Default)]
pub(crate) struct StatusCell {
    pub(crate) uid: AtomicU64,
    pub(crate) state: AtomicU8,
    pub(crate) position: AtomicUsize,
    pub(crate) len: AtomicUsize,
    pub(crate) layers: AtomicUsize,
    pub(crate) content_rev: AtomicU64,
}

pub(crate) fn state_code(state: LooperState) -> u8 {
    match state {
        LooperState::Empty => 0,
        LooperState::Recording => 1,
        LooperState::Playing => 2,
        LooperState::Overdubbing => 3,
        LooperState::Stopped => 4,
    }
}

pub(crate) fn state_from_code(code: u8) -> LooperState {
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
    pub(crate) ops: ArrayQueue<LooperOp>,
    pub(crate) retired: ArrayQueue<Box<[f32]>>,
    pub(crate) status: Vec<StatusCell>,
    /// Longest loop, in frames, at this runtime's live sample rate. The
    /// control thread reads it to size the buffers it allocates.
    pub(crate) max_frames: usize,
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
                content_rev: c.content_rev.load(Ordering::Relaxed),
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
                    content_rev: c.content_rev.load(Ordering::Relaxed),
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
            dst.content_rev
                .store(src.content_rev.load(Ordering::Relaxed), Ordering::Relaxed);
        }
    }
}
