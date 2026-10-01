//! Responsibility: describes the state a split keeps between callbacks.

use domain::ids::BlockId;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::{align_delay, AlignDelay, MAX_ALIGN_SAMPLES};
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::lanes::PathLanes;
use crate::runtime_split::latency::{path_latency, path_latency_ceiling};
use crate::runtime_split::mix::PathKnobs;
use crate::runtime_state::{BlockRuntimeNode, SEGMENT_FRAME_CAPACITY};

/// One split's runtime (#328, spec §4.1, §11.4): every path, the buffer each
/// one runs in, one alignment delay line per path, the knobs and the lanes
/// the paths run on. Everything
/// is allocated here, at build; the callback only reuses it.
pub(crate) struct SplitRuntimeState {
    /// `true` for Split → Mix; `false` for Y, whose paths meet at unity.
    pub(crate) mixes: bool,
    pub(crate) paths: Vec<Vec<BlockRuntimeNode>>,
    /// Each path's frames for the current callback.
    pub(crate) bufs: Vec<Vec<AudioFrame>>,
    pub(crate) aligns: Vec<AlignDelay>,
    /// Each path's knob values for the current callback (scratch).
    pub(crate) values: Vec<PathKnobs>,
    pub(crate) knobs: SplitKnobs,
    /// The threads that run every path but the first.
    pub(crate) lanes: PathLanes,
}

impl SplitRuntimeState {
    pub(crate) fn new(
        mixes: bool,
        paths: Vec<Vec<BlockRuntimeNode>>,
        knobs: SplitKnobs,
        block_id: &BlockId,
    ) -> Self {
        // Room for any toggle inside a path: the longest a path can get.
        let room = paths
            .iter()
            .map(|path| path_latency_ceiling(path))
            .max()
            .unwrap_or(0);
        let count = paths.len();
        let mut state = Self {
            mixes,
            paths,
            bufs: (0..count)
                .map(|_| Vec::with_capacity(SEGMENT_FRAME_CAPACITY))
                .collect(),
            aligns: (0..count)
                .map(|_| AlignDelay::with_capacity(room))
                .collect(),
            values: vec![PathKnobs::neutral(); count],
            knobs,
            lanes: PathLanes::spawn(count, &block_id.0),
        };
        if state.refresh_alignment() {
            log::warn!(
                "split '{}': its paths differ by more than {} samples — aligned up to the cap",
                block_id.0,
                MAX_ALIGN_SAMPLES
            );
        }
        state
    }

    /// Line the paths up again from the blocks enabled now; `true` when the
    /// cap cut a difference. Runs at build and, after a toggle inside a path,
    /// on the audio thread: no allocation, no log.
    pub(crate) fn refresh_alignment(&mut self) -> bool {
        let longest = self
            .paths
            .iter()
            .map(|path| path_latency(path))
            .max()
            .unwrap_or(0);
        let mut clamped = false;
        for (path, align) in self.paths.iter().zip(self.aligns.iter_mut()) {
            let (delay, cut) = align_delay(longest, path_latency(path));
            clamped |= cut;
            align.set_delay(delay);
        }
        clamped
    }

    /// Carry the previous build's delay history and path buffers into this
    /// one, path by path, so a rebuild (a knob or a path edit) does not
    /// restart a delayed path from silence. The previous path nodes were
    /// already handed to the reuse pool.
    pub(crate) fn adopt_history(&mut self, previous: SplitRuntimeState) {
        let SplitRuntimeState { aligns, bufs, .. } = previous;
        for (align, old) in self.aligns.iter_mut().zip(aligns) {
            align.adopt_history(old);
        }
        for (buf, old) in self.bufs.iter_mut().zip(bufs) {
            if old.capacity() >= buf.capacity() {
                *buf = old;
            }
        }
    }
}

#[cfg(test)]
#[path = "runtime_split_state_tests.rs"]
mod tests;
