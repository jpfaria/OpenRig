//! #785 — seeking the DI loop of an isolated DI runtime.
//!
//! A gapless DI re-arm builds a NEW routed runtime while the current render
//! keeps sounding, and hands over mid-loop. The incoming runtime therefore has
//! to render from the position the listener will be at when the hand-off
//! lands, not from the top of the loop (which would restart the take).

use std::sync::atomic::Ordering;

use crate::runtime::ChainRuntimeState;

impl ChainRuntimeState {
    /// Place the DI loop read head at `pos` (frames, wrapped by the reader).
    /// `set_di_loop` rewinds to 0, so this is called after it.
    pub fn set_di_loop_pos(&self, pos: usize) {
        self.di_loop_pos.store(pos, Ordering::Relaxed);
    }
}
