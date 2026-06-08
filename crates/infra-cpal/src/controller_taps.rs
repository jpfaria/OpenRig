//! Runtime-query and tap-subscription methods on
//! `ProjectRuntimeController`.
//!
//! These are pulled out of `controller.rs` so the main file stays under
//! the 600-LOC cap. They share the same `impl ProjectRuntimeController`
//! block as the lifecycle methods (start/sync/upsert/teardown);
//! splitting per `impl` keeps them callable through the same
//! `controller.method()` API consumers already use.
//!
//! Concerns covered here:
//! - `poll_stream`, `poll_errors` — drain UI-facing diagnostics from
//!   the per-chain runtimes.
//! - `measured_latency_ms` / `arm_latency_probe` / `cancel_latency_probe`
//!   — the latency-probe pipeline.
//! - `subscribe_input_tap` / `subscribe_stream_tap` — give the UI
//!   read-only fan-outs of pre-FX or per-stream stereo audio.
//! - `prune_dead_input_taps` / `prune_dead_stream_taps` /
//!   `stream_count` / `set_output_muted` — small queries the UI calls
//!   on each frame or close handler.

use std::sync::Arc;

use domain::ids::ChainId;

use crate::controller::ProjectRuntimeController;

impl ProjectRuntimeController {
    pub fn poll_stream(
        &self,
        block_id: &domain::ids::BlockId,
    ) -> Option<Vec<block_core::StreamEntry>> {
        for (_, runtime) in &self.runtime_graph.chains {
            if let Some(entries) = runtime.poll_stream(block_id) {
                return Some(entries);
            }
        }
        None
    }

    /// Drains and returns all block errors that occurred since the last call.
    pub fn poll_errors(&self) -> Vec<engine::runtime::BlockError> {
        self.runtime_graph
            .chains
            .values()
            .flat_map(|runtime| runtime.poll_errors())
            .collect()
    }

    /// Returns the measured real-time latency in milliseconds for a given chain.
    pub fn measured_latency_ms(&self, chain_id: &ChainId) -> Option<f32> {
        // Issue #350: the latency probe injects/detects the beep ONLY on
        // the primary signal path — `process_input_f32`/`process_output_f32`
        // gate the probe on `index == 0`. `runtime_for_chain` returns the
        // lowest-group (group 0) per-input runtime, which IS the primary
        // input. So measuring that runtime is correct, not a fan-out gap:
        // it reports the user-visible round-trip the probe is designed to
        // measure. Secondary input devices share the same chain DSP cost
        // profile and are not separately probed by design.
        self.runtime_graph
            .runtime_for_chain(chain_id)
            .map(|runtime| runtime.measured_latency_ms())
    }

    /// Arms a latency probe on the given chain: the next input callback
    /// injects a short beep, and the first output callback that sees it
    /// updates `measured_latency_ms`. No-op if the chain has no runtime.
    pub fn arm_latency_probe(&self, chain_id: &ChainId) {
        // Issue #350: arm the primary-input runtime (group 0). The probe is
        // intentionally single-path — `process_input_f32` only injects the
        // beep when `input_index == 0`, so arming any other per-input
        // runtime would never fire. This measures the user-visible round
        // trip, which is what the UI displays.
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            runtime.arm_latency_probe();
        }
    }

    /// Cancels any in-flight latency probe on the given chain. The UI
    /// calls this when the on-screen probe display window expires so a
    /// probe that never produced a detection does not stay armed.
    pub fn cancel_latency_probe(&self, chain_id: &ChainId) {
        // Issue #350: cancels the primary-input runtime (group 0) — the
        // only one `arm_latency_probe` ever arms (see the note there).
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            runtime.cancel_latency_probe();
        }
    }

    /// Subscribe to raw pre-FX samples from a chain's input. See
    /// [`engine::runtime::ChainRuntimeState::subscribe_input_tap`] for the
    /// full contract. Returns an empty `Vec` if the chain has no runtime
    /// or `input_index` is out of range.
    ///
    /// `input_index` is the GLOBAL stream index across the chain's
    /// per-input runtimes (same convention as
    /// [`Self::subscribe_stream_tap`] and [`Self::stream_count`]); the
    /// dispatch walks `runtimes_for(chain_id)`, subtracts each runtime's
    /// local stream count, and forwards to the runtime hosting the
    /// segment using its real cpal-callback group index. Issue #557:
    /// before this translation existed, an input_index of 1 on a chain
    /// that hosts both streams in one runtime (e.g. two mono guitars on
    /// the same device) fell back to the first runtime and passed `1`
    /// as the runtime-side filter, which the runtime's group-0 callback
    /// never matched — silencing every consumer (meter, tuner) for the
    /// secondary streams.
    ///
    /// `total_channels` should be at least `max(subscribed_channels) + 1`;
    /// any extra slots are unused. Pass the actual device-side channel
    /// count if you know it, otherwise compute it from the input entry.
    pub fn subscribe_input_tap(
        &self,
        chain_id: &ChainId,
        input_index: usize,
        total_channels: usize,
        subscribed_channels: &[usize],
        capacity_per_channel: usize,
    ) -> Vec<Arc<engine::spsc::SpscRing<f32>>> {
        let mut remaining = input_index;
        for runtime in self.runtime_graph.runtimes_for(chain_id) {
            let local_count = runtime.stream_count();
            if remaining < local_count {
                let cpal_input_index = runtime
                    .input_routing_for_stream(remaining)
                    .map(|(cpal_idx, _, _)| cpal_idx)
                    .unwrap_or(remaining);
                return runtime.subscribe_input_tap(
                    cpal_input_index,
                    total_channels,
                    subscribed_channels,
                    capacity_per_channel,
                );
            }
            remaining -= local_count;
        }
        Vec::new()
    }

    /// Subscribe to a per-stream INPUT meter tap. Mirrors
    /// [`Self::subscribe_stream_tap`] in shape: takes a GLOBAL
    /// `stream_index` in `0..stream_count(cid)` and returns a single
    /// ring containing the device-side audio that this stream's
    /// segment actually reads — honouring the chain's input endpoint
    /// channels.
    ///
    /// Issue #557: replaces the meter's old call pattern of
    /// `subscribe_input_tap(cid, i, 1, &[0], cap)`. That pattern is
    /// wrong on two counts:
    ///
    /// 1. When several streams share one per-input runtime (e.g. two
    ///    mono guitars on one device), each stream's segment lives at
    ///    the SAME local cpal-callback group index (always 0 for a
    ///    one-device chain). Passing the global stream index as the
    ///    tap's `input_index` filter makes every tap past index 0
    ///    silent because the runtime's callback only fires with the
    ///    cpal group index.
    /// 2. The hardcoded `&[0]` ignores the chain's input endpoint
    ///    channels: a chain wired to device channel 1 still ends up
    ///    sampling channel 0 of the interleaved frame.
    ///
    /// This method translates the global `stream_index` to the
    /// `(per-input runtime, local segment)` pair (same walk
    /// `subscribe_stream_tap` already uses) and then asks the runtime
    /// for the segment's real cpal-callback index and device channels
    /// before subscribing the tap. For the meter we surface the first
    /// device channel only; multi-channel inputs keep their full
    /// `subscribe_input_tap` access for advanced consumers (tuner /
    /// spectrum / analyzers) that still want every channel separately.
    pub fn subscribe_stream_input_tap(
        &self,
        chain_id: &ChainId,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> Option<Arc<engine::spsc::SpscRing<f32>>> {
        let mut remaining = stream_index;
        for runtime in self.runtime_graph.runtimes_for(chain_id) {
            let local_count = runtime.stream_count();
            if remaining < local_count {
                let (cpal_input_index, total_channels, device_channels) =
                    runtime.input_routing_for_stream(remaining)?;
                let first_channel = *device_channels.first()?;
                let mut rings = runtime.subscribe_input_tap(
                    cpal_input_index,
                    total_channels,
                    &[first_channel],
                    capacity_per_channel,
                );
                return rings.pop();
            }
            remaining -= local_count;
        }
        None
    }

    /// Drop input taps with no surviving consumer handles across all
    /// chains. Cheap; intended to be called from a UI timer or window
    /// close handler.
    pub fn prune_dead_input_taps(&self) {
        for runtime in self.runtime_graph.chains.values() {
            runtime.prune_dead_input_taps();
        }
    }

    /// Subscribe to a per-stream stereo tap (post-FX, pre-mixdown) on a
    /// chain. Returns `[l_ring, r_ring]` — both rings always present
    /// because every stream is internally stereo. See
    /// [`engine::runtime::ChainRuntimeState::subscribe_stream_tap`] for
    /// the full contract. Returns rings that will stay empty if the
    /// chain has no runtime.
    pub fn subscribe_stream_tap(
        &self,
        chain_id: &ChainId,
        stream_index: usize,
        capacity_per_channel: usize,
    ) -> Option<[Arc<engine::spsc::SpscRing<f32>>; 2]> {
        // Issue #350 phase 3: a chain owns N per-input runtimes, each with
        // its own LOCAL segment indices starting at 0. `stream_count`
        // (used by the UI to drive `0..count`) is the SUM across runtimes,
        // so `stream_index` is GLOBAL. Walk the per-input runtimes in
        // group order, subtracting each runtime's stream count, and
        // subscribe on the runtime that owns this global index using its
        // local index — otherwise a tap for a secondary input device would
        // wrongly attach to the primary runtime (which never produces that
        // segment) and stay silent.
        let mut remaining = stream_index;
        for runtime in self.runtime_graph.runtimes_for(chain_id) {
            let local_count = runtime.stream_count();
            if remaining < local_count {
                return Some(runtime.subscribe_stream_tap(remaining, capacity_per_channel));
            }
            remaining -= local_count;
        }
        None
    }

    /// How many streams (input pipelines) a chain currently runs. Empty
    /// chains and chains without a runtime return 0.
    pub fn stream_count(&self, chain_id: &ChainId) -> usize {
        // Issue #350: a chain may own N per-input runtimes; the chain's
        // total stream count is the sum across all of them.
        self.runtime_graph
            .runtimes_for(chain_id)
            .iter()
            .map(|runtime| runtime.stream_count())
            .sum()
    }

    /// Total audio-thread deadline overruns (xruns) counted across this
    /// chain's per-input runtimes (issue #670). Read by the GUI meter timer
    /// (~30 Hz) to drive the per-chain overload indicator, and exposed via
    /// `QueryKind` for MCP / gRPC parity. Unknown / runtime-less chains
    /// return 0.
    pub fn chain_xrun_count(&self, chain_id: &ChainId) -> u64 {
        self.runtime_graph
            .runtimes_for(chain_id)
            .iter()
            .map(|runtime| runtime.xrun_count())
            .sum()
    }

    /// Drop stream taps with no surviving consumer handles across all chains.
    pub fn prune_dead_stream_taps(&self) {
        for runtime in self.runtime_graph.chains.values() {
            runtime.prune_dead_stream_taps();
        }
    }

    /// Toggle the output-mute flag on every chain runtime. When true,
    /// the output stage zeros every frame — used by the Tuner window
    /// so the user can tune silently. Auto-cleared on window close.
    pub fn set_output_muted(&self, mute: bool) {
        for runtime in self.runtime_graph.chains.values() {
            runtime.set_output_muted(mute);
        }
    }

    /// Publish (or clear) the DI loop for a single chain.
    ///
    /// `Some(arc)` arms every `ChainRuntimeState` associated with `chain_id`
    /// so the audio thread picks it up on the next callback. `None` disarms
    /// them — audio returns to live input immediately.
    ///
    /// Called from the adapter-gui wiring after
    /// `Event::ChainDiLoopEnabledChanged` is received; the `Arc<DiLoop>` is
    /// retrieved from the dispatcher's ephemeral store (not persisted).
    pub fn set_chain_di_loop(
        &self,
        chain_id: &ChainId,
        di: Option<Arc<engine::DiLoop>>,
    ) {
        for runtime in self.runtime_graph.runtimes_for(chain_id) {
            runtime.set_di_loop(di.clone());
        }
    }

    /// Returns `true` when at least one `ChainRuntimeState` for `chain_id`
    /// has an active DI loop armed (i.e. `has_di_loop() == true`).
    ///
    /// Called by the GUI meter timer (~30 Hz) to refresh the
    /// `ProjectChainItem.di_loop_playing` flag in the VecModel row.
    /// Read-only; no audio-thread interaction.
    pub fn chain_has_di_loop(&self, chain_id: &ChainId) -> bool {
        self.runtime_graph
            .runtimes_for(chain_id)
            .iter()
            .any(|rt| rt.has_di_loop())
    }
}
