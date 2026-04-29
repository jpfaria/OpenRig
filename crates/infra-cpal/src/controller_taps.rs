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
        self.runtime_graph
            .chains
            .get(chain_id)
            .map(|runtime| runtime.measured_latency_ms())
    }

    /// Arms a latency probe on the given chain: the next input callback
    /// injects a short beep, and the first output callback that sees it
    /// updates `measured_latency_ms`. No-op if the chain has no runtime.
    pub fn arm_latency_probe(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.chains.get(chain_id) {
            runtime.arm_latency_probe();
        }
    }

    /// Cancels any in-flight latency probe on the given chain. The UI
    /// calls this when the on-screen probe display window expires so a
    /// probe that never produced a detection does not stay armed.
    pub fn cancel_latency_probe(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.chains.get(chain_id) {
            runtime.cancel_latency_probe();
        }
    }

    /// Subscribe to raw pre-FX samples from a chain's input. See
    /// [`engine::runtime::ChainRuntimeState::subscribe_input_tap`] for the
    /// full contract. Returns an empty `Vec` if the chain has no runtime.
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
        match self.runtime_graph.chains.get(chain_id) {
            Some(runtime) => runtime.subscribe_input_tap(
                input_index,
                total_channels,
                subscribed_channels,
                capacity_per_channel,
            ),
            None => Vec::new(),
        }
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
        self.runtime_graph
            .chains
            .get(chain_id)
            .map(|runtime| runtime.subscribe_stream_tap(stream_index, capacity_per_channel))
    }

    /// How many streams (input pipelines) a chain currently runs. Empty
    /// chains and chains without a runtime return 0.
    pub fn stream_count(&self, chain_id: &ChainId) -> usize {
        self.runtime_graph
            .chains
            .get(chain_id)
            .map(|runtime| runtime.stream_count())
            .unwrap_or(0)
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
}
