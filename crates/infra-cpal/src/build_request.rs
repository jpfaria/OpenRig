//! Issue #672 — the worker-runnable build entry.
//!
//! `BuildRequest` is the owned, `Send` payload the frontend thread hands to the
//! [`ControlWorker`](crate::ControlWorker); [`build_chain_runtime`] performs the
//! heavy step (NAM model loads, segment + route assembly) on the worker and
//! returns the fresh runtime as an `Arc`, ready to be published into a
//! [`LiveRuntimeSlot`](crate::LiveRuntimeSlot).

use std::sync::Arc;

use anyhow::Result;
use engine::runtime::{build_chain_runtime_state, ChainRuntimeState};
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
}

/// Build a fresh chain runtime from `req`. Worker-runnable: this is the heavy
/// DSP-assembly step that must not run on the frontend thread (issue #672).
///
/// # Errors
/// Propagates any failure from `engine`'s chain-runtime assembly (e.g. a model
/// that fails to load).
pub fn build_chain_runtime(req: &BuildRequest) -> Result<Arc<ChainRuntimeState>> {
    let state = build_chain_runtime_state(&req.chain, req.sample_rate, &req.buffer_sizes)?;
    Ok(Arc::new(state))
}
