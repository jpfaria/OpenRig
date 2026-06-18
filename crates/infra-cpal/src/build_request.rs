//! Issue #672 — the worker-runnable build entry.
//!
//! `BuildRequest` is the owned, `Send` payload the frontend thread hands to the
//! [`ControlWorker`](crate::ControlWorker); [`build_chain_runtime`] performs the
//! heavy step (NAM model loads, segment + route assembly) on the worker and
//! returns the fresh runtime as an `Arc`, ready to be published into a
//! [`LiveRuntimeSlot`](crate::LiveRuntimeSlot).

use std::sync::Arc;

use anyhow::Result;
use domain::io_binding::IoBinding;
use engine::runtime::{build_per_input_runtime_states_with_bindings, ChainRuntimeState};
use project::chain::Chain;

/// Owned, `Send` description of a chain runtime to (re)build off the frontend
/// thread. Everything needed by [`build_chain_runtime`] is owned so it can move
/// to the worker thread.
pub struct BuildRequest {
    /// The chain definition to build a runtime for.
    pub chain: Chain,
    /// Target sample rate (Hz).
    pub sample_rate: f32,
    /// Per-input elastic buffer targets (device buffer sizes).
    pub buffer_sizes: Vec<usize>,
    /// Issue #716 — the per-machine I/O binding registry (`AppConfig.io_bindings`),
    /// owned so it can cross to the worker thread. A chain whose Input/Output
    /// blocks carry a non-empty `io` is routed PER BINDING against this
    /// registry; a pure-legacy chain (empty `io`) ignores it and keeps the
    /// `entries`-based path. Empty for callers with no registry.
    pub io_bindings: Vec<IoBinding>,
}

/// Build the fresh per-entry chain runtimes from `req`. Worker-runnable: this
/// is the heavy DSP-assembly step that must not run on the frontend thread
/// (issue #672). Issue #703: one isolated runtime per input-entry group —
/// the caller publishes each into its `(chain, group)` slot; single-entry
/// chains get exactly one `(0, runtime)` pair (the legacy shape).
///
/// Issue #716: a chain whose ports carry a non-empty `io` is routed PER BINDING
/// against `req.io_bindings`; a pure-legacy chain (empty `io`) keeps the
/// `entries`-based path byte-identical. The branch lives in the engine seam
/// `build_per_input_runtime_states_with_bindings` so this stays a thin worker
/// wrapper.
///
/// # Errors
/// Propagates any failure from `engine`'s chain-runtime assembly (e.g. a model
/// that fails to load).
pub fn build_chain_runtime(req: &BuildRequest) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    build_per_input_runtime_states_with_bindings(
        &req.chain,
        req.sample_rate,
        &req.buffer_sizes,
        &req.io_bindings,
    )
}
