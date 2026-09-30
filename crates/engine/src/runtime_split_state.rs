//! Responsibility: describes the state a split keeps between callbacks.

use domain::ids::BlockId;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::{plan_alignment, AlignDelay, AlignPlan, MAX_ALIGN_SAMPLES};
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::latency::{path_latency, path_latency_ceiling};
use crate::runtime_state::{BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

/// One split's runtime (#328, spec §4.1): both paths, the buffer path B runs
/// in, the two alignment delay lines and the knobs. Everything is allocated
/// here, at build; the callback only reuses it.
pub(crate) struct SplitRuntimeState {
    /// `true` for Split → Mix; `false` for Y → A/B, whose paths meet at unity.
    pub(crate) mixes: bool,
    pub(crate) a: Vec<BlockRuntimeNode>,
    pub(crate) b: Vec<BlockRuntimeNode>,
    /// Path B's frames for the current callback.
    pub(crate) b_buf: Vec<AudioFrame>,
    pub(crate) align_a: AlignDelay,
    pub(crate) align_b: AlignDelay,
    pub(crate) knobs: SplitKnobs,
}

impl SplitRuntimeState {
    pub(crate) fn new(
        mixes: bool,
        a: Vec<BlockRuntimeNode>,
        b: Vec<BlockRuntimeNode>,
        knobs: SplitKnobs,
        block_id: &BlockId,
    ) -> Self {
        // Room for any toggle inside a path: the longest a path can get.
        let room = path_latency_ceiling(&a).max(path_latency_ceiling(&b));
        let mut state = Self {
            mixes,
            a,
            b,
            b_buf: Vec::with_capacity(SEGMENT_FRAME_CAPACITY),
            align_a: AlignDelay::with_capacity(room),
            align_b: AlignDelay::with_capacity(room),
            knobs,
        };
        if state.refresh_alignment().clamped {
            log::warn!(
                "split '{}': its paths differ by more than {} samples — aligned up to the cap",
                block_id.0,
                MAX_ALIGN_SAMPLES
            );
        }
        state
    }

    /// Line the paths up again from the blocks enabled now. Runs at build
    /// and, after a toggle inside a path, on the audio thread: no
    /// allocation, no log.
    pub(crate) fn refresh_alignment(&mut self) -> AlignPlan {
        let plan = plan_alignment(path_latency(&self.a), path_latency(&self.b));
        self.align_a.set_delay(plan.delay_a);
        self.align_b.set_delay(plan.delay_b);
        plan
    }

    /// Carry the previous build's delay history and path-B buffer into this
    /// one, so a rebuild (a knob or a path edit) does not restart the delayed
    /// path from silence. The previous path nodes were already handed to the
    /// reuse pool.
    pub(crate) fn adopt_history(&mut self, previous: SplitRuntimeState) {
        let SplitRuntimeState {
            align_a,
            align_b,
            b_buf,
            ..
        } = previous;
        self.align_a.adopt_history(align_a);
        self.align_b.adopt_history(align_b);
        if b_buf.capacity() >= self.b_buf.capacity() {
            self.b_buf = b_buf;
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_state_tests.rs"]
mod tests;
