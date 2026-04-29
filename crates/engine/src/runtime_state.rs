//! Internal state types for the chain runtime — block nodes, processor
//! variants, fade state, scratch buffers, output routing.
//!
//! Lifted out of `runtime.rs` (slice 2 of the Phase 2 split) so the
//! parent file shrinks toward the < 600 LOC target.
//!
//! These types are PASSED INTO the audio thread (held in
//! `ChainRuntimeState`'s `processing` Mutex / `output_routes` ArcSwap)
//! but their methods are mostly setup-time (constructors, snapshots).
//! The two methods that DO run per-callback are marked `#[inline]`
//! preemptively — same lesson as slice 1 — so they keep being inlined
//! across the new module boundary:
//!
//!   - `InputCallbackScratch::reset_for_callback` — called once per
//!     audio callback in `process_input_f32`.
//!   - `SelectRuntimeState::selected_node_mut` — called per callback
//!     for any segment that contains a Select block.
//!
//! Visibility:
//!   - `BlockError` is `pub` (re-exported from `runtime` so
//!     `engine::runtime::BlockError` keeps working in `infra-cpal`
//!     and `adapter-console`).
//!   - Everything else is `pub(crate)` — these are runtime internals
//!     used only from `runtime.rs`, `stream_tap.rs`, and the test
//!     modules.

use std::collections::HashMap;
use std::sync::Arc;

use block_core::{AudioChannelLayout, StreamHandle};
use domain::ids::BlockId;
use project::block::AudioBlock;
use project::chain::ChainOutputMixdown;

use crate::runtime_audio_frame::{AudioFrame, AudioProcessor, ElasticBuffer, ProcessorScratch};

/// An error produced by a block processor during audio processing.
#[derive(Debug, Clone)]
pub struct BlockError {
    pub block_id: BlockId,
    pub message: String,
}

pub(crate) struct InputProcessingState {
    pub(crate) input_read_layout: AudioChannelLayout,
    pub(crate) processing_layout: AudioChannelLayout,
    pub(crate) input_channels: Vec<usize>,
    pub(crate) blocks: Vec<BlockRuntimeNode>,
    pub(crate) frame_buffer: Vec<AudioFrame>,
    /// Remaining frames of fade-in after a rebuild (0 = no fade active).
    pub(crate) fade_in_remaining: usize,
    /// Which output route indices this input/segment should push frames to.
    /// Empty means push to ALL output routes (legacy behaviour).
    pub(crate) output_route_indices: Vec<usize>,
    /// When this segment originated from a split-mono entry (one
    /// `InputBlock` with `mode: mono` and N channels), this stores N —
    /// the total number of split siblings sharing the same original entry.
    /// The fan-out then scales this segment's contribution by 1/N before
    /// summing into `mixed_per_route`, preventing the unity-gain sum of N
    /// loud streams from saturating the output limiter. The mono→stereo
    /// upmix stays the historical broadcast (`Stereo([s, s])`) — the rule
    /// "mono in → stereo out is broadcast to both channels" is preserved.
    /// `None` for stereo / single-channel mono / dual-mono / Insert
    /// returns — they contribute at unity gain. (Issue #350.)
    pub(crate) split_mono_sibling_count: Option<usize>,
}

pub(crate) struct ChainProcessingState {
    pub(crate) input_states: Vec<InputProcessingState>,
    /// Maps CPAL input_index → Vec of input_states indices to process.
    pub(crate) input_to_segments: Vec<Vec<usize>>,
    /// Pre-allocated scratch buffers used by `process_input_f32`, indexed by
    /// CPAL input_index. Reused across callbacks to avoid per-callback
    /// allocations in the RT hot path.
    pub(crate) input_scratches: Vec<InputCallbackScratch>,
}

/// Scratch buffers reused across audio callbacks for a single input_index.
/// Each Vec/HashMap keeps its allocated capacity between callbacks; clearing
/// leaves the backing storage in place.
#[derive(Default)]
pub(crate) struct InputCallbackScratch {
    /// Mixed audio frames keyed by output route index, accumulated across
    /// segments for the current callback.
    pub(crate) mixed_per_route: HashMap<usize, Vec<AudioFrame>>,
    /// Output route Arcs snapshotted from `runtime.output_routes` via
    /// ArcSwap for this callback — no lock held.
    pub(crate) route_arcs: Vec<(usize, Arc<OutputRoutingState>)>,
    /// Buffer written by `process_single_segment` with the processed frames
    /// of the current segment before they are mixed into `mixed_per_route`.
    pub(crate) segment_processed: Vec<AudioFrame>,
    /// Output route indices for the current segment, refreshed per segment.
    pub(crate) segment_route_indices: Vec<usize>,
    /// Segment indices belonging to the current input_index, refreshed per
    /// callback from `input_to_segments`.
    pub(crate) segment_indices: Vec<usize>,
}

impl InputCallbackScratch {
    /// Called at the top of every audio callback in `process_input_f32`.
    /// Hot path — `#[inline]` preserves the same-module inlining the
    /// compiler used to give us before this code crossed a module
    /// boundary.
    #[inline]
    pub(crate) fn reset_for_callback(&mut self) {
        for buf in self.mixed_per_route.values_mut() {
            buf.clear();
        }
        self.route_arcs.clear();
        self.segment_processed.clear();
        self.segment_route_indices.clear();
        self.segment_indices.clear();
    }
}

pub(crate) struct OutputRoutingState {
    pub(crate) output_channels: Vec<usize>,
    pub(crate) output_mixdown: ChainOutputMixdown,
    pub(crate) buffer: ElasticBuffer,
}

pub(crate) enum RuntimeProcessor {
    Audio(AudioProcessor),
    Select(SelectRuntimeState),
    Bypass,
}

impl RuntimeProcessor {
    /// Stable label of the processor variant — for diagnostics. Keeps the
    /// `AudioProcessor` and `SelectRuntimeState` types private to the
    /// runtime module while letting sibling modules (e.g. the latency
    /// probe) classify nodes without pattern-matching on the variants.
    pub(crate) fn kind_label(&self) -> &'static str {
        match self {
            RuntimeProcessor::Audio(_) => "audio",
            RuntimeProcessor::Select(_) => "select",
            RuntimeProcessor::Bypass => "bypass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FadeState {
    /// Fully active — no fade in progress.
    Active,
    /// Transitioning from bypass → active. frames_remaining counts down.
    FadingIn { frames_remaining: usize },
    /// Transitioning from active → bypass. frames_remaining counts down.
    FadingOut { frames_remaining: usize },
    /// Fully bypassed — no audio processing needed.
    Bypassed,
}

pub(crate) struct BlockRuntimeNode {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) instance_serial: u64,
    pub(crate) block_id: BlockId,
    pub(crate) block_snapshot: AudioBlock,
    pub(crate) input_layout: AudioChannelLayout,
    pub(crate) output_layout: AudioChannelLayout,
    pub(crate) scratch: ProcessorScratch,
    pub(crate) processor: RuntimeProcessor,
    pub(crate) stream_handle: Option<StreamHandle>,
    pub(crate) fade_state: FadeState,
    /// Set to true if this block panicked during audio processing.
    /// Once faulted, the block is permanently bypassed to prevent repeated crashes.
    pub(crate) faulted: bool,
}

pub(crate) struct SelectRuntimeState {
    pub(crate) selected_block_id: BlockId,
    pub(crate) options: Vec<BlockRuntimeNode>,
}

pub(crate) struct ProcessorBuildOutcome {
    pub(crate) processor: AudioProcessor,
    pub(crate) output_layout: AudioChannelLayout,
    pub(crate) stream_handle: Option<StreamHandle>,
}

impl SelectRuntimeState {
    /// Hot path — called per callback for any segment containing a Select
    /// block. `#[inline]` so the dispatch through this method stays as
    /// cheap as it was when this code lived in `runtime.rs`.
    #[inline]
    pub(crate) fn selected_node_mut(&mut self) -> Option<&mut BlockRuntimeNode> {
        self.options
            .iter_mut()
            .find(|option| option.block_id == self.selected_block_id)
    }
}
