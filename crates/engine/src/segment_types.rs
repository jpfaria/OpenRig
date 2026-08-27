//! Responsibility: describes one segment of a chain.

use crate::runtime_endpoints::InputEntry;

/// An `Output` block sitting BETWEEN effect blocks (issue #85): it emits the
/// signal as processed UP TO ITS POSITION while the chain keeps flowing to the
/// blocks after it. Non-destructive — a tap, not a cut.
#[derive(Clone, Copy)]
pub(crate) struct MidOutputTap {
    /// Index of the `Output` block within `chain.blocks`.
    pub(crate) offset: usize,
    /// Route index of the endpoint it writes to (position among the chain's
    /// resolved outputs, the same order the runtime numbers routes with).
    pub(crate) route_idx: usize,
}

/// A tap resolved against ONE segment: how many of that segment's blocks run
/// before the signal is emitted, and where it goes.
#[derive(Clone, Copy)]
pub(crate) struct SegmentTap {
    /// Number of this segment's blocks processed before the tap emits.
    pub(crate) blocks_before: usize,
    pub(crate) route_idx: usize,
}

/// Describes a chain segment: an input source, its effect blocks, and its
/// output targets.
#[allow(dead_code)]
pub(crate) struct ChainSegment {
    pub(crate) input: InputEntry,
    pub(crate) cpal_input_index: usize,
    pub(crate) block_indices: Vec<usize>,
    pub(crate) output_route_indices: Vec<usize>,
    /// Mid-chain output taps this segment emits while processing (#85).
    pub(crate) mid_output_taps: Vec<SegmentTap>,
    /// Inherited from the originating effective input. `Some(N)` when this
    /// segment came from a split-mono entry (one InputBlock with
    /// `mode: mono` and >1 channel) and owns output channel position N.
    /// `None` for stereo / dual-mono / single-channel-mono / Insert-return
    /// segments — they keep the historical broadcast/sum behaviour.
    pub(crate) split_mono_sibling_count: Option<usize>,
    /// RAW input-entry index this segment's effective input came from
    /// (issue #703). The runtime graph partitions segments by this id:
    /// distinct raw entries become isolated runtimes even on one shared
    /// physical device, while split-mono siblings (same raw entry) stay
    /// together so the pinned g02/g03 sum-before-limiter math holds.
    pub(crate) entry_group: usize,
}
