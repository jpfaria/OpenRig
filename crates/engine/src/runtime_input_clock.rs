//! Responsibility: records when each input's current buffer was captured.

use std::sync::atomic::Ordering;

use crate::runtime_chain_state::ChainRuntimeState;

/// Inputs per chain whose capture time is tracked; a higher index is ignored.
pub(crate) const STAMPED_INPUTS: usize = 16;

impl ChainRuntimeState {
    /// Note the host-clock time (ns) the buffer about to be processed on
    /// `input_index` was captured. Called from the input's own worker before
    /// it processes the buffer; one relaxed store.
    #[inline]
    pub fn note_input_capture_ns(&self, input_index: usize, capture_ns: u64) {
        if let Some(slot) = self.input_capture_ns.get(input_index) {
            slot.store(capture_ns, Ordering::Relaxed);
        }
    }

    /// The capture time last noted for `input_index`, 0 when none is known.
    #[inline]
    pub fn input_capture_ns(&self, input_index: usize) -> u64 {
        self.input_capture_ns
            .get(input_index)
            .map_or(0, |slot| slot.load(Ordering::Relaxed))
    }
}
