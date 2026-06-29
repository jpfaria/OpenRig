//! Issue #672 — the worker-runnable build entry.
//!
//! `BuildRequest` is the owned, `Send` payload the frontend thread hands to the
//! [`ControlWorker`](crate::ControlWorker); [`build_chain_runtime`] performs the
//! heavy step (NAM model loads, segment + route assembly) on the worker and
//! returns the fresh runtime as an `Arc`, ready to be published into a
//! [`LiveRuntimeSlot`](crate::LiveRuntimeSlot).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use engine::runtime::{build_per_input_runtime_states, ChainRuntimeState};
use project::chain::Chain;

/// Owned, `Send` description of a chain runtime to (re)build off the frontend
/// thread. Everything needed by [`build_chain_runtime`] is owned so it can move
/// to the worker thread.
pub struct BuildRequest {
    /// The chain definition to build a runtime for.
    pub chain: Chain,
    /// Representative / fallback rate (Hz) — first binding's rate (#736).
    pub sample_rate: f32,
    /// Per-input-device rates (#736). Each isolated runtime is clocked at its
    /// own device's rate; missing device falls back to `sample_rate`.
    pub device_sample_rates: HashMap<DeviceId, f32>,
    /// Per-input elastic buffer targets (device buffer sizes).
    pub buffer_sizes: Vec<usize>,
    /// Model A (#716): the per-machine I/O binding registry, owned so it can
    /// move to the worker thread. Device endpoints resolve from this, not from
    /// the chain — see [`engine::runtime_endpoints::resolve_chain_io`].
    pub io_bindings: Vec<IoBinding>,
}

/// Build the fresh per-entry chain runtimes from `req`. Worker-runnable: this
/// is the heavy DSP-assembly step that must not run on the frontend thread
/// (issue #672). Issue #703: one isolated runtime per input-entry group —
/// the caller publishes each into its `(chain, group)` slot; single-entry
/// chains get exactly one `(0, runtime)` pair (the legacy shape).
///
/// # Errors
/// Propagates any failure from `engine`'s chain-runtime assembly (e.g. a model
/// that fails to load).
pub fn build_chain_runtime(req: &BuildRequest) -> Result<Vec<(usize, Arc<ChainRuntimeState>)>> {
    build_per_input_runtime_states(
        &req.chain,
        req.sample_rate,
        &req.device_sample_rates,
        &req.buffer_sizes,
        &req.io_bindings,
    )
}
