//! The `ChainRuntimeState` root struct and its core accessors (split out of
//! `runtime_state.rs`, which kept growing past the file cap).
//!
//! `runtime_state.rs` keeps the per-segment / per-block state types this
//! struct owns; the tap-subscription half of the same `impl` lives in
//! `runtime_state_taps.rs`. Pure move — same public API, re-exported from
//! `crate::runtime_state` so every existing path keeps resolving.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::{ArcSwap, ArcSwapOption};
use block_core::StreamHandle;
use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;

use crate::di_loop::DiLoop;
use crate::input_tap::InputTap;
use crate::runtime_state::{BlockError, ChainProcessingState, OutputRoutingState};
use crate::stream_tap::StreamTap;

/// Root state for one chain runtime. Holds the per-callback state behind
/// a `Mutex` (write contention only happens on chain rebuild), the per-route
/// output state behind `ArcSwap` (RT callback reads without locking), and
/// the assorted atomics + queues that drive UI ↔ audio-thread communication
/// (probe state, error queue, drain flag, output mute, observability taps).
pub struct ChainRuntimeState {
    /// Issue #703: set when this runtime is one isolated per-entry runtime
    /// — `(entry_group, cpal_input_index)`. `entry_group` is the raw
    /// `InputEntry` index this runtime owns (the `RuntimeGraph` key);
    /// `cpal_input_index` is the device stream that feeds it — SHARED with
    /// sibling runtimes when two entries read the same physical device.
    /// `None` for whole-chain runtimes (probe, offline render, JACK).
    /// Written once at graph assembly, before the state is shared.
    pub(crate) owned_entry: Option<(usize, usize)>,
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
    /// Lock-free mirror of `processing.input_states.len()` (issue #580).
    /// The meter polling timer calls `stream_count` at 30 Hz on the GUI
    /// thread — reading it through the `processing` Mutex contends with
    /// the audio thread's `process_input_f32 → try_lock`, and a failed
    /// try_lock there means the callback emits silence (audible click at
    /// small buffer sizes). `input_states.len()` only changes on runtime
    /// build / rebuild in `runtime_graph`, so a single `AtomicUsize`
    /// updated at those two sites is sufficient — readers never block
    /// the audio thread. Relaxed ordering: the value is purely
    /// informational for the GUI's subscription loop; a reader briefly
    /// seeing the old count just defers an extra subscription to the
    /// next tick. No synchronisation with audio-thread state needed.
    pub(crate) stream_count: AtomicUsize,
    /// Lock-free producer/consumer queue for pending block-toggle
    /// requests (issue #580 follow-up). The GUI's
    /// `BlockCommand::ToggleBlockEnabled` handler calls `set_block_enabled`,
    /// which pushes `(block_id, enabled)` here without taking any lock.
    /// The audio thread drains and applies the toggle inside its
    /// `process_input_f32` `try_lock`'d section (one cheap walk over
    /// `input_states.blocks`, in place). Result: the GUI thread NEVER
    /// holds `processing`, so the audio thread's `try_lock` never fails
    /// because of a user toggle — the click the user heard on every
    /// block on/off is gone. Capacity 64 is far above the user's
    /// per-frame click rate (the audio drain runs at hundreds of Hz),
    /// so a queue overflow in practice would mean the audio thread has
    /// stalled — which is a louder bug than a dropped toggle. The
    /// `set_block_enabled` API still surfaces such overflows as `Err`.
    pub(crate) pending_block_toggles: ArrayQueue<(BlockId, bool)>,
    /// Lock-free mirror of the block ids whose live node is a
    /// `RuntimeProcessor::Bypass` — blocks built while disabled (or whose
    /// build faulted) carry no real DSP. Re-enabling such a block CANNOT
    /// be served by the queued in-place fade fast path (there is no
    /// processor to fade in), so `set_block_enabled` declines synchronously
    /// and the caller falls back to a full `upsert_chain` rebuild.
    ///
    /// Issue #580 regression: once the fast path started *queueing* every
    /// toggle and always returning `Ok`, the synchronous decline that used
    /// to trigger that rebuild was lost — re-enabling a born-disabled block
    /// only spammed "has no live processor" to `error_queue` and the block
    /// never came back. This mirror restores the decline without taking the
    /// `processing` lock on the GUI thread (`ArcSwap::load` is wait-free).
    /// Swapped at the same build/rebuild sites that update `stream_count`,
    /// so it always reflects the current set of live nodes.
    pub(crate) bypass_block_ids: ArcSwap<HashSet<BlockId>>,
    /// Ephemeral per-chain virtual DI loop source (issue #614). `None` means
    /// use the live device input; `Some` means feed samples from the
    /// pre-decoded buffer instead. Published off the audio thread via
    /// `ArcSwapOption` so the audio thread reads it lock-free on every
    /// `process_input_f32` callback. `set_di_loop` also resets
    /// `di_loop_pos` to 0 so playback always starts from the beginning.
    pub(crate) di_loop: ArcSwapOption<DiLoop>,
    /// Playback cursor into the active `di_loop` buffer (frame index).
    /// Advanced once per input frame in `process_input_f32` by the
    /// single audio-thread writer; wrapped modulo `DiLoop::len()`.
    /// Relaxed ordering is sufficient: there is exactly one writer (the
    /// audio thread) and one reader (also the audio thread), so no
    /// cross-thread synchronisation is needed. `set_di_loop` resets this
    /// to 0 from the off-thread caller; the audio thread will read the
    /// new value on its next callback.
    pub(crate) di_loop_pos: AtomicUsize,
    /// #323: control ↔ audio channel for this chain's loopers (op queue,
    /// buffer-return queue and the lock-free status mirror the UI reads).
    pub(crate) loopers: crate::looper_bank::LooperShared,
    /// Issue #670 — audio-thread deadline accounting. The output callback
    /// records its own wall-clock cost via `record_callback_load`: every
    /// buffer whose processing exceeded the device period increments
    /// `xrun_count`, and `peak_load_ppm` keeps the worst `elapsed/period`
    /// ratio (in parts-per-million) since the last `reset_load_stats`.
    /// Both are read off the audio thread by the GUI meter timer and by
    /// `QueryKind` (MCP / gRPC parity) to surface an overload warning
    /// instead of letting the dropout crackle unexplained. Relaxed
    /// ordering: the values are purely informational for the UI, with one
    /// audio-thread writer and one off-thread reader. See
    /// [`crate::runtime_load`].
    pub(crate) xrun_count: AtomicU64,
    pub(crate) peak_load_ppm: AtomicU64,
    /// The sample rate (Hz) this runtime was built at — the rate its streams
    /// actually run at. Set once at construction, never mutated, so a plain
    /// `f32` read is safe from the audio thread. Issue #723: the live probe
    /// beep in `process_input_f32` reads this instead of assuming 48000, so
    /// the 1 kHz tone is synthesized at the true device rate.
    pub(crate) sample_rate: f32,
}

impl ChainRuntimeState {
    /// The sample rate (Hz) this runtime runs at, captured at build time.
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Issue #703: the cpal input-stream index that feeds this runtime,
    /// when it is a per-entry isolated runtime. Two entries reading the
    /// same physical device are two runtimes with the SAME index — the
    /// cpal layer fans that device's single callback out to all of them.
    /// `None` for whole-chain runtimes (probe, offline render, JACK).
    pub fn input_cpal_index(&self) -> Option<usize> {
        self.owned_entry.map(|(_, cpal_idx)| cpal_idx)
    }

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

    /// Toggle the output-mute flag. When `true`, `process_output_f32`
    /// zeros every output frame. Cheap (single atomic store) and safe
    /// to call from any thread.
    pub fn set_output_muted(&self, mute: bool) {
        self.output_muted.store(mute, Ordering::Relaxed);
    }

    pub fn is_output_muted(&self) -> bool {
        self.output_muted.load(Ordering::Relaxed)
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

    /// Publish a new DI loop (or clear it) and reset the playback cursor
    /// to 0. Safe to call from any thread. The audio thread will pick up
    /// the change on its next `process_input_f32` callback (lock-free
    /// ArcSwapOption load).
    pub fn set_di_loop(&self, di: Option<Arc<DiLoop>>) {
        self.di_loop.store(di);
        self.di_loop_pos.store(0, Ordering::Relaxed);
    }

    /// Returns `true` if a virtual DI loop is currently active for this
    /// chain (i.e. `process_input_f32` will read from the buffer rather
    /// than the live device input). Lock-free (single ArcSwapOption load).
    pub fn has_di_loop(&self) -> bool {
        self.di_loop.load().is_some()
    }

    /// Frame length of the armed DI loop, or `None` when disarmed. The
    /// audio thread reads one loop frame per output frame at THIS runtime's
    /// rate, so the length must match a resample to `sample_rate` — a loop
    /// built at a different rate plays at the wrong speed (#749 slow-mo).
    /// Read-only; used by tests and the DI parity query.
    pub fn di_loop_len(&self) -> Option<usize> {
        self.di_loop.load().as_ref().map(|d| d.len())
    }
}
