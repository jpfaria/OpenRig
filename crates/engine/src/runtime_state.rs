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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;
use block_core::{AudioChannelLayout, StreamHandle};
use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;
use project::block::AudioBlock;
use project::chain::ChainOutputMixdown;

use crate::input_tap::InputTap;
use crate::runtime_audio_frame::{AudioFrame, AudioProcessor, ElasticBuffer, ProcessorScratch};
use crate::spsc::SpscRing;
use crate::stream_tap::StreamTap;

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
    /// Pre-allocated buffer for fade crossfade dry-signal capture.
    /// Issue #400 bug #4: replaces `frames.to_vec()` (which alloc'd on
    /// every audio callback during fade) with a clear()+extend pattern
    /// that reuses capacity. Vec::clear() does NOT deallocate; subsequent
    /// extends only realloc if capacity is exceeded — which after the
    /// first frame is no longer the case for fixed-buffer audio backends.
    pub(crate) fade_dry_buffer: Vec<AudioFrame>,
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

/// Root state for one chain runtime. Holds the per-callback state behind
/// a `Mutex` (write contention only happens on chain rebuild), the per-route
/// output state behind `ArcSwap` (RT callback reads without locking), and
/// the assorted atomics + queues that drive UI ↔ audio-thread communication
/// (probe state, error queue, drain flag, output mute, observability taps).
pub struct ChainRuntimeState {
    pub(crate) processing: Mutex<ChainProcessingState>,
    /// Per-route output state. Swapped atomically on chain rebuild so the
    /// RT callback sees a fresh snapshot without taking any lock.
    pub(crate) output_routes: ArcSwap<Vec<Arc<OutputRoutingState>>>,
    /// Stream handles published by block processors, polled by UI thread.
    pub(crate) stream_handles: Mutex<HashMap<BlockId, StreamHandle>>,
    /// Errors posted by the audio thread, drained by the UI thread.
    /// Lock-free SPSC bounded queue: audio thread `push` is wait-free,
    /// UI thread `pop` is wait-free. Audio thread never blocks on UI
    /// contention. When full, audio drops new errors silently — the
    /// queue is sized for any plausible burst.
    pub(crate) error_queue: ArrayQueue<BlockError>,
    /// Monotonic reference used to derive `u64` nanos for latency probes.
    /// Captured at state creation.
    pub(crate) created_at: std::time::Instant,
    /// Nanos-since-`created_at` of the moment a probe beep was injected
    /// into the input stream. Written by `process_input_f32` when a probe
    /// transitions Armed → Fired; read by `process_output_f32` when the
    /// beep is detected at the output, to compute the measured latency.
    pub(crate) last_input_nanos: AtomicU64,
    /// Measured end-to-end latency (ns) of the last completed probe.
    /// Exposed to the UI via `measured_latency_ms()`.
    pub(crate) measured_latency_nanos: AtomicU64,
    /// Latency probe state machine: Idle / Armed / Fired. Transitions are
    /// Idle → Armed (user click), Armed → Fired (input callback injects
    /// the beep), Fired → Idle (output callback detects the beep).
    pub(crate) probe_state: AtomicU8,
    /// When true, the audio callback must not call any block processors.
    /// Set before deactivating the JACK client to prevent use-after-free
    /// in C++ NAM destructors (terminate called without active exception).
    pub(crate) draining: AtomicBool,
    /// Per-channel sample taps published to consumers (Tuner / Spectrum
    /// windows). Empty by default. Hot-swapped via ArcSwap so the audio
    /// thread reads without locking. See `crate::input_tap::InputTap`.
    pub(crate) input_taps: ArcSwap<Vec<Arc<InputTap>>>,
    /// Per-stream sample taps published to consumers (Spectrum window).
    /// A "stream" is one `InputProcessingState` — one input feeding one
    /// parallel pipeline through the chain — so each tap publishes the
    /// post-FX, pre-mixdown stereo signal of a single guitar / mic /
    /// keyboard. Subscribing per-stream (instead of per-output-channel)
    /// keeps the analyzer separate per input source even when several
    /// inputs share an output device.
    ///
    /// The publish point is *before* the device-side `output_muted`
    /// zero-fill, so muting the audio output does not blank the analyzer.
    pub(crate) stream_taps: ArcSwap<Vec<Arc<StreamTap>>>,
    /// When true, the output stage zeros every frame before publishing.
    /// Toggled by any consumer that needs a silent output (e.g. the
    /// Tuner window). Auto-cleared on consumer close.
    pub(crate) output_muted: AtomicBool,
    /// Output volume da chain em percentual (issue #440). 100 = unity,
    /// 200 = 2× (+6 dB), 50 = metade (-6 dB). Multiplicado no master output
    /// do `process_output_f32` como controle único do preset/chain. Atomic
    /// pra suportar mudança em runtime via UI sem destruir o chain runtime.
    /// f32 armazenado como u32 bits (Relaxed ordering é suficiente — não há
    /// sincronização com outras escritas).
    pub(crate) volume_pct_bits: AtomicU32,
}

impl ChainRuntimeState {
    /// Signal the audio callback to stop processing blocks.
    /// Must be called before deactivating JACK or dropping block processors.
    pub fn set_draining(&self) {
        self.draining.store(true, Ordering::Release);
    }

    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Acquire)
    }

    /// Output volume da chain em percentual (issue #440). Linear ratio
    /// aplicado no master output do `process_output_f32`. Audio-thread safe.
    pub fn volume_pct(&self) -> f32 {
        f32::from_bits(self.volume_pct_bits.load(Ordering::Relaxed))
    }

    /// Atualiza o volume da chain. Chamado pela UI/control thread quando
    /// o usuário muda o controle. Audio thread vê o novo valor na próxima
    /// callback (single atomic load por iteração).
    pub fn set_volume_pct(&self, pct: f32) {
        self.volume_pct_bits.store(pct.to_bits(), Ordering::Relaxed);
    }

    /// Re-arm the audio callback after a teardown-and-rebuild cycle that
    /// reuses this `ChainRuntimeState` (the Arc is kept alive in
    /// `RuntimeGraph` across `infra_cpal::teardown_active_chain_for_rebuild`).
    /// Without this reset the new CPAL/JACK streams attached to the same
    /// runtime would see `is_draining()` return `true` on every callback and
    /// silence audio indefinitely until the chain is fully removed and
    /// re-added — issue #316.
    pub fn clear_draining(&self) {
        self.draining.store(false, Ordering::Release);
    }

    /// Subscribe to raw pre-FX samples from one input. Returns one
    /// [`SpscRing`] handle per requested channel, in the same order as
    /// `subscribed_channels`. The audio thread starts pushing samples on
    /// the next callback. Drop the returned handles to unsubscribe — the
    /// tap is removed from the registry on the next subscription change.
    ///
    /// `total_channels` should match the input's actual interleaved channel
    /// count (i.e. the `input_total_channels` argument the audio callback
    /// receives). `capacity_per_channel` is the SPSC ring depth in samples;
    /// pick a value comfortably larger than (consumer poll period) ×
    /// (sample rate).
    pub fn subscribe_input_tap(
        &self,
        input_index: usize,
        total_channels: usize,
        subscribed_channels: &[usize],
        capacity_per_channel: usize,
    ) -> Vec<Arc<SpscRing<f32>>> {
        let (tap, handles) = InputTap::new(
            input_index,
            total_channels,
            subscribed_channels,
            capacity_per_channel,
        );
        let mut new_taps: Vec<Arc<InputTap>> =
            self.input_taps.load_full().iter().cloned().collect();
        new_taps.push(Arc::new(tap));
        self.input_taps.store(Arc::new(new_taps));
        handles
    }

    /// Toggle the output-mute flag. When `true`, `process_output_f32`
    /// zeros every output frame. Cheap (single atomic store) and safe
    /// to call from any thread.
    pub fn set_output_muted(&self, mute: bool) {
        self.output_muted.store(mute, Ordering::Relaxed);
    }

    pub fn is_output_muted(&self) -> bool {
        self.output_muted.load(Ordering::Relaxed)
    }

    /// Drop input taps that no longer have any external `SpscRing` handles
    /// kept by consumers. Cheap to call; intended for periodic cleanup
    /// from a UI timer (e.g. when the tuner window closes).
    ///
    /// Detection works because the audio thread only borrows the rings via
    /// the `Arc<InputTap>`; if no consumer holds a handle, the channel
    /// `Arc`s have refcount 1 (only the `InputTap` holds them).
    pub fn prune_dead_input_taps(&self) {
        let current = self.input_taps.load_full();
        let mut kept: Vec<Arc<InputTap>> = Vec::with_capacity(current.len());
        let mut changed = false;
        for tap in current.iter() {
            let has_consumer = tap
                .channel_rings
                .iter()
                .filter_map(|r| r.as_ref())
                .any(|ring| Arc::strong_count(ring) > 1);
            if has_consumer {
                kept.push(Arc::clone(tap));
            } else {
                changed = true;
            }
        }
        if changed {
            self.input_taps.store(Arc::new(kept));
        }
    }

    /// Subscribe a consumer to the post-FX, pre-mixdown stereo samples
    /// of stream `stream_index` (one input pipeline) in this chain.
    /// Returns `[l_ring, r_ring]` — both rings always present because
    /// every stream is internally stereo (mono inputs are upmixed
    /// before the FX chain).
    ///
    /// Same lock-free semantics as `subscribe_input_tap`. The dispatch
    /// happens before the device-side `output_muted` zero-fill so muting
    /// the audio output does not blank the analyzer.
    pub fn subscribe_stream_tap(
        &self,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> [Arc<SpscRing<f32>>; 2] {
        let (tap, handles) = StreamTap::new(stream_index, capacity_per_channel);
        let mut new_taps: Vec<Arc<StreamTap>> =
            self.stream_taps.load_full().iter().cloned().collect();
        new_taps.push(Arc::new(tap));
        self.stream_taps.store(Arc::new(new_taps));
        handles
    }

    /// Drop stream taps whose consumer handles have all been released.
    /// Mirrors `prune_dead_input_taps`.
    pub fn prune_dead_stream_taps(&self) {
        let current = self.stream_taps.load_full();
        let mut kept: Vec<Arc<StreamTap>> = Vec::with_capacity(current.len());
        let mut changed = false;
        for tap in current.iter() {
            // A consumer holds an Arc to either or both rings; if neither
            // ring has any external Arc, this tap is dead.
            let has_consumer =
                Arc::strong_count(&tap.l_ring) > 1 || Arc::strong_count(&tap.r_ring) > 1;
            if has_consumer {
                kept.push(Arc::clone(tap));
            } else {
                changed = true;
            }
        }
        if changed {
            self.stream_taps.store(Arc::new(kept));
        }
    }

    /// How many streams this chain currently runs (one per
    /// `InputProcessingState`). Used by the UI to know how many
    /// per-stream taps to subscribe.
    pub fn stream_count(&self) -> usize {
        self.processing
            .lock()
            .map(|p| p.input_states.len())
            .unwrap_or(0)
    }

    /// For a given LOCAL stream index in this runtime
    /// (`0..stream_count()`), return the input routing metadata needed
    /// to subscribe its per-stream INPUT meter tap:
    /// `(cpal_input_index, total_channels, device_channels)` where:
    ///
    /// - `cpal_input_index` is the runtime's cpal-callback group index
    ///   the tap must filter on (the value `process_input_f32` is
    ///   called with by the cpal stream that owns this segment).
    /// - `total_channels` is `max(device_channels) + 1`, sized so the
    ///   `InputTap`'s `channel_rings` Vec covers every subscribed
    ///   channel. Higher cpal-side counts are tolerated by the tap
    ///   dispatch loop (it skips channels past its own length).
    /// - `device_channels` is the set of interleaved-frame channels
    ///   this segment reads from (one entry for mono / split-mono /
    ///   single-channel mono, two for stereo / dual-mono). Issue #557:
    ///   the UI must subscribe THESE channels, not a default `[0]`,
    ///   so a chain wired to device channel 1 actually sees channel
    ///   1's signal in its meter ring.
    ///
    /// Returns `None` if `local_stream_index >= stream_count()` or no
    /// cpal group hosts that segment (degenerate, only happens during
    /// a teardown-rebuild race).
    pub fn input_routing_for_stream(
        &self,
        local_stream_index: usize,
    ) -> Option<(usize, usize, Vec<usize>)> {
        let processing = self.processing.lock().ok()?;
        let input_state = processing.input_states.get(local_stream_index)?;
        let cpal_input_index = processing
            .input_to_segments
            .iter()
            .position(|seg_idxs| seg_idxs.contains(&local_stream_index))?;
        let device_channels = input_state.input_channels.clone();
        let total_channels = device_channels
            .iter()
            .copied()
            .max()
            .map(|m| m + 1)
            .unwrap_or(0);
        Some((cpal_input_index, total_channels, device_channels))
    }

    /// Returns stream data for a block by ID, or None if not found or empty.
    ///
    /// The inner read is wait-free (`ArcSwap::load`); only the outer
    /// `stream_handles` HashMap takes a brief lock against rebuild paths
    /// (also UI thread, never the RT callback).
    pub fn poll_stream(&self, block_id: &BlockId) -> Option<Vec<block_core::StreamEntry>> {
        let handles = self.stream_handles.lock().ok()?;
        let handle = handles.get(block_id)?;
        let entries = handle.load();
        if entries.is_empty() {
            None
        } else {
            Some((**entries).clone())
        }
    }

    /// Drains and returns all block errors posted since the last call.
    ///
    /// Wait-free against the audio thread: each `pop()` is lock-free, and
    /// the audio thread is never blocked even if the UI is mid-drain.
    pub fn poll_errors(&self) -> Vec<BlockError> {
        let mut out = Vec::new();
        while let Some(err) = self.error_queue.pop() {
            out.push(err);
        }
        out
    }
}

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
