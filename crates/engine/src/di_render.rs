//! #771: build the DI's isolated runtime, routed to the CHOSEN output.
//!
//! The DI plays on a fresh, independent copy of the chain's block graph
//! (invariant #4 — nothing shared with the guitar runtime). #716 routes a
//! binding's inputs only to that SAME binding's outputs and the armed loop
//! substitutes segment 0 only (#699), so the chain's head/tail I/O is
//! REDUCED to the chosen output's binding — the loop then feeds the route
//! the chosen output drains. Blocks are untouched: the DSP is identical.
//!
//! The runtime is STEPPED off the audio thread by the DI worker
//! (`infra-cpal::di_stream`), paced by ring backpressure — never by a free
//! clock (the sleep-paced worker→output routing tried in #717 drifted and
//! was reverted, `f1131725e`) and never by a full pre-render (a 75 s loop
//! through a NAM chain took minutes before the first sample — the owner's
//! "play and no sound").

use std::sync::Arc;

use anyhow::{anyhow, Result};
use domain::io_binding::IoBinding;
use project::chain::{Chain, DiOutputRef};

use crate::runtime::{build_chain_runtime_state, ChainRuntimeState};
use crate::DiPcm;

/// The DI's isolated runtime plus everything the worker needs to drain the
/// chosen output's route: the route index inside the (reduced) runtime and
/// the drain layout the route writes.
pub struct RoutedDiRuntime {
    pub runtime: Arc<ChainRuntimeState>,
    /// Output route index within the reduced runtime.
    pub output_index: usize,
    /// Interleaved drain-buffer layout: channel offsets the route writes.
    pub drain_left: usize,
    pub drain_right: usize,
    pub drain_width: usize,
    /// One loop period at the runtime's rate, in frames.
    pub loop_len: usize,
}

/// Build the isolated DI runtime for `chain`, reduced to the binding of the
/// chosen output (`None`/stale → the main output; a chain with no bound
/// outputs keeps its implicit default route).
pub fn build_routed_di_runtime(
    chain: &Chain,
    registry: &[IoBinding],
    di_output: Option<&DiOutputRef>,
    output_rate: u32,
    pcm: &DiPcm,
) -> Result<RoutedDiRuntime> {
    let groups: Vec<_> = crate::runtime_endpoints::resolve_chain_io_by_binding(chain, registry)
        .into_iter()
        .filter(|g| !g.outputs.is_empty())
        .collect();
    let target = di_output
        .and_then(|target| {
            let group = groups.iter().find(|g| g.binding_id == target.binding_id)?;
            let binding = registry.iter().find(|b| b.id == group.binding_id)?;
            let local = binding
                .outputs
                .iter()
                .position(|ep| ep.name == target.endpoint)?;
            Some((group.binding_id.clone(), local))
        })
        .or_else(|| groups.first().map(|g| (g.binding_id.clone(), 0)));

    let (reduced, output_index) = match target {
        Some((binding_id, local_index)) => {
            let mut reduced = chain.clone();
            reduced.io_binding_ids = vec![binding_id];
            (reduced, local_index)
        }
        // No bound outputs: the chain's implicit default route 0.
        None => (chain.clone(), 0),
    };

    let runtime = Arc::new(build_chain_runtime_state(
        &reduced,
        output_rate as f32,
        &[],
        registry,
    )?);
    let di_loop = pcm.to_loop_at(output_rate);
    let loop_len = di_loop.len();
    if loop_len == 0 {
        return Err(anyhow!("DI loop is empty"));
    }
    runtime.set_di_loop(Some(Arc::new(di_loop)));

    let (drain_left, drain_right, drain_width) = {
        let routes = runtime.output_routes.load();
        let route = routes
            .get(output_index)
            .ok_or_else(|| anyhow!("chain has no output route {output_index}"))?;
        let left = route.output_channels.first().copied().unwrap_or(0);
        let right = route.output_channels.get(1).copied().unwrap_or(left);
        let width = route.output_channels.iter().copied().max().unwrap_or(0) + 1;
        (left, right, width)
    };

    Ok(RoutedDiRuntime {
        runtime,
        output_index,
        drain_left,
        drain_right,
        drain_width,
        loop_len,
    })
}
