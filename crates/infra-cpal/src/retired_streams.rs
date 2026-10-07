//! Responsibility: keeps the stream sets a chain replaced alive until the set that replaced them has taken over.
//!
//! #1081: the old set is not dropped when the new one is installed — it keeps
//! playing, fading out against the new set (`stream_handover`), and goes only
//! once the new set plays on its own. A new set that never gets heard (its
//! device died) does not keep the old one forever: past [`RETIRE_DEADLINE`]
//! the old set goes anyway. Dropping a set closes its device streams, so it
//! happens here, on the control side, never on an audio thread.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::active_runtime::ActiveChainRuntime;
use crate::stream_handover::StreamHandover;

/// How long a replaced set may outlive the install of the set replacing it.
pub(crate) const RETIRE_DEADLINE: Duration = Duration::from_secs(2);

/// A stream set's place in its chain's handover: its own fade, and the sets
/// it replaced, still fading out.
pub(crate) struct StreamSwap {
    pub(crate) handover: Arc<StreamHandover>,
    retiring: Vec<(ActiveChainRuntime, Instant)>,
}

impl Default for StreamSwap {
    /// A set that replaces nothing.
    fn default() -> Self {
        Self {
            handover: StreamHandover::cold(),
            retiring: Vec::new(),
        }
    }
}

impl StreamSwap {
    /// A set that replaces the live streams of its chain.
    pub(crate) fn replacing() -> Self {
        Self {
            handover: StreamHandover::replacing(),
            retiring: Vec::new(),
        }
    }

    /// `previous` was replaced by the set holding this swap at `now`.
    pub(crate) fn retire(&mut self, previous: ActiveChainRuntime, now: Instant) {
        previous
            .swap
            .handover
            .retire_into(Arc::clone(&self.handover));
        self.retiring.push((previous, now));
    }

    /// Drop every replaced set this one has taken over from, or that outlived
    /// [`RETIRE_DEADLINE`]. Returns how many went.
    pub(crate) fn reap(&mut self, now: Instant) -> usize {
        let before = self.retiring.len();
        if self.handover.has_taken_over() {
            self.retiring.clear();
        } else {
            self.retiring
                .retain(|(_, at)| now.duration_since(*at) < RETIRE_DEADLINE);
        }
        before - self.retiring.len()
    }

    /// Replaced sets still playing.
    #[cfg(test)]
    pub(crate) fn retiring_len(&self) -> usize {
        self.retiring.len()
    }
}

#[cfg(test)]
#[path = "retired_streams_tests.rs"]
mod tests;
