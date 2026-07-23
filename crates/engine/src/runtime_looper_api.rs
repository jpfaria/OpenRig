//! Issue #323 — the control-thread facade of a chain's loopers.
//!
//! Everything here is wait-free: pushing an op is an `ArrayQueue` push,
//! reading a status is a handful of relaxed atomic loads. No caller ever
//! takes the `processing` lock the audio thread try-locks (#580).

use crate::looper_bank::{LooperOp, LooperStatus};
use crate::runtime_state::ChainRuntimeState;

impl ChainRuntimeState {
    /// Longest loop this runtime can hold, in frames at its live sample rate.
    /// The control thread sizes the layer buffers it allocates from this.
    pub fn looper_max_frames(&self) -> usize {
        self.loopers.max_frames()
    }

    /// Queue a looper op for the audio thread. `Err` gives the op back when
    /// the queue is full — that means the audio thread stopped draining.
    pub fn push_looper_op(&self, op: LooperOp) -> Result<(), LooperOp> {
        self.loopers.push(op)
    }

    /// State of one looper, or `None` when this runtime holds no such looper.
    pub fn looper_status(&self, uid: u64) -> Option<LooperStatus> {
        self.loopers.status(uid)
    }

    /// State of every looper this runtime holds, in slot order.
    pub fn looper_statuses(&self) -> Vec<LooperStatus> {
        self.loopers.statuses()
    }

    /// Collect the layer buffers the audio thread handed back and drop them
    /// here — freeing memory is forbidden on the audio thread (invariant #8).
    pub fn drain_retired_layers(&self) -> Vec<Box<[f32]>> {
        self.loopers.drain_retired()
    }
}
