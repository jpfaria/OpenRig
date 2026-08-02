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
use std::sync::{Arc, Mutex};

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
    /// #85: `Output` blocks sitting BETWEEN this segment's effect blocks.
    /// Each emits the signal as processed up to its own position while the
    /// segment keeps running down to its end-of-chain route.
    pub(crate) mid_output_taps: Vec<crate::runtime_segments::SegmentTap>,
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
    /// #85: does an armed DI loop play on THIS pipeline? Every pipeline fed by
    /// the chain's first input entry does — each writes its own route, so the
    /// loop is heard once per route, never summed twice. Split-mono siblings
    /// (a different entry sharing one route) stay silent, which is the rule
    /// #699 introduced.
    pub(crate) plays_di_loop: bool,
    /// #454-T5: previous pipeline decaying in parallel after a switch.
    /// `None` in steady state ⇒ behaviour byte-identical to pre-#454-T5.
    pub(crate) outgoing: Option<Box<OutgoingTail>>,
}

pub(crate) struct ChainProcessingState {
    pub(crate) input_states: Vec<InputProcessingState>,
    /// Maps CPAL input_index → Vec of input_states indices to process.
    pub(crate) input_to_segments: Vec<Vec<usize>>,
    /// Pre-allocated scratch buffers used by `process_input_f32`, indexed by
    /// CPAL input_index. Reused across callbacks to avoid per-callback
    /// allocations in the RT hot path.
    pub(crate) input_scratches: Vec<InputCallbackScratch>,
    /// #323: this chain's loopers. Owned by this runtime alone; the audio
    /// thread already holds `&mut` to the processing state, so the slots need
    /// no lock of their own.
    pub(crate) looper_bank: crate::looper_bank::LooperBank,
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
    /// Whether the signal reaching this block is effectively mono — i.e. a
    /// stereo bus whose two channels carry the identical sample (a mono
    /// source broadcast to `Stereo([s, s])`, preserved by every preceding
    /// block). Issue #588: a `DualMono` model under effective-mono content
    /// is built as a single mono processor instead of one per channel
    /// (bit-identical output, half the model footprint). Part of the reuse
    /// identity: if this flips, the block must rebuild (mono ↔ dual).
    pub(crate) content_mono: bool,
    pub(crate) output_layout: AudioChannelLayout,
    pub(crate) scratch: ProcessorScratch,
    pub(crate) processor: RuntimeProcessor,
    pub(crate) stream_handle: Option<StreamHandle>,
    pub(crate) fade_state: FadeState,
    /// Pre-allocated buffer for fade crossfade dry-signal capture.
    /// Issue #400 bug #4: replaces `frames.to_vec()` (which alloc'd on
    /// every audio callback during fade) with a clear()+extend pattern
    /// that reuses capacity. Vec::clear() does NOT deallocate; subsequent
    /// extends only realloc if capacity is exceeded — which after the
    /// first frame is no longer the case for fixed-buffer audio backends.
    pub(crate) fade_dry_buffer: Vec<AudioFrame>,
    /// Set to true if this block panicked during audio processing OR if
    /// its runtime build failed at setup time. Once faulted, the block is
    /// permanently bypassed to prevent repeated crashes.
    pub(crate) faulted: bool,
    /// Human-readable explanation when [`Self::faulted`] is set due to a
    /// build-time failure. `None` for runtime panics (the panic site logs
    /// separately) and for healthy blocks. Surfaced via
    /// `engine::offline::RenderOutcome::faulted_blocks` so offline-render
    /// callers can refuse to claim success when a block was silently
    /// bypassed. Issue #574.
    pub(crate) fault_reason: Option<String>,
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

/// Number of frames to fade in after a chain rebuild to avoid clicks/pops.
/// Lives next to `FadeState` because it parameterises that state machine.
pub(crate) const FADE_IN_FRAMES: usize = 128;

/// #454-T5 spillover window: after a preset/scene switch the previous
/// pipeline keeps processing **silence** (so its delay/reverb tail rings
/// out) while being equal-power faded out over this many frames, then it is
/// dropped. ~0.75 s @ 48 kHz — long enough for musical tails, bounded so the
/// extra CPU is a transient, not a steady cost (hierarchy: CPU < sound).
pub(crate) const SPILLOVER_FRAMES: usize = 36_000;

/// The decaying previous pipeline kept alive in-segment during a switch.
/// SPSC-safe: it is summed into the segment's own `frame_buffer` *before*
/// the single per-route push, so there is still exactly one producer per
/// output ring. Built entirely off the audio thread.
pub(crate) struct OutgoingTail {
    pub(crate) blocks: Vec<BlockRuntimeNode>,
    pub(crate) frames_remaining: usize,
    /// Pre-allocated silence/work buffer (no alloc on the audio thread).
    pub(crate) scratch: Vec<AudioFrame>,
}

/// The root runtime struct lives in its own module (file cap); re-exported
/// here so `crate::runtime_state::ChainRuntimeState` keeps resolving.
pub use crate::runtime_chain_state::ChainRuntimeState;

/// Acquire a `Mutex` even if a prior panic poisoned it (issue #415).
///
/// PoisonError is recoverable: it indicates that some other thread panicked
/// while holding the lock, but the underlying data is still accessible.
/// In this codebase the only writer is the chain-rebuild path, which
/// overwrites `input_states` and `input_to_segments` wholesale, so a
/// partially inconsistent state is healed by the very next call.
///
/// Aborting the process on poison is strictly worse than logging and
/// continuing — the original panic was already reported (via `log::error!`
/// from `apply_block_processor` or upstream).
///
/// Audio-thread callsites still use `try_lock` and must NOT call this —
/// they treat `Err` (whether poison or contention) as "skip this callback".
pub(crate) fn lock_recover<'a, T>(
    mutex: &'a Mutex<T>,
    name: &'static str,
) -> std::sync::MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        log::error!("{name} mutex was poisoned by a prior panic — recovering and continuing");
        poisoned.into_inner()
    })
}
