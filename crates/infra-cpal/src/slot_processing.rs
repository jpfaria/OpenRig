//! Issue #672 — the audio-thread processing seam.
//!
//! The CPAL input/output callbacks call these helpers, which read the chain's
//! *live* runtime through a [`LiveRuntimeSlot`] every buffer instead of holding
//! a fixed `Arc` captured at stream-build time. That single wait-free
//! `slot.load()` (an `Arc` refcount bump — no heap, no lock, no syscall) is the
//! only cost added to the audio thread, preserving invariant #8, and lets the
//! control worker swap a rebuilt runtime in without tearing the stream down.

use std::sync::Arc;

use engine::runtime::{process_input_f32, process_output_f32_mixed, ChainRuntimeState};

use crate::LiveRuntimeSlot;

/// Wrap each per-group runtime in a fresh [`LiveRuntimeSlot`] (issue #672).
///
/// The stream callbacks capture these slot handles and read them live, and the
/// controller stores the same slots so the control worker can publish a rebuilt
/// runtime into them without tearing the stream down.
#[must_use]
pub fn build_chain_slots(
    runtimes: &[(usize, Arc<ChainRuntimeState>)],
) -> Vec<(usize, LiveRuntimeSlot)> {
    runtimes
        .iter()
        .map(|(group, runtime)| (*group, LiveRuntimeSlot::new(Arc::clone(runtime))))
        .collect()
}

/// Process one input buffer through the chain's live input runtime.
///
/// Wait-free: one `slot.load()` then the existing `process_input_f32`.
pub fn process_input_buffer(
    slot: &LiveRuntimeSlot,
    input_index: usize,
    data: &[f32],
    input_total_channels: usize,
) {
    process_input_f32(&slot.load(), input_index, data, input_total_channels);
}

/// Mix the chain's live per-group output runtimes into `out`.
///
/// `loaded` and `scratch` are caller-owned buffers captured once in the stream
/// callback (sized to the group count / output length), so this allocates
/// nothing per buffer: `loaded.clear()` + `push` reuses capacity and each
/// `slot.load()` only bumps an `Arc` refcount.
pub fn process_output_buffer(
    slots: &[LiveRuntimeSlot],
    loaded: &mut Vec<Arc<ChainRuntimeState>>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    loaded.clear();
    for slot in slots {
        loaded.push(slot.load());
    }
    process_output_f32_mixed(loaded, output_index, out, output_total_channels, scratch);
}
