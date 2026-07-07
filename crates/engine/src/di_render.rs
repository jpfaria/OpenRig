//! #771: off-line pre-render of the DI loop through a fresh, independent copy
//! of the chain's block graph, at the CHOSEN output's rate.
//!
//! The rendered buffer is what the output device's callback plays back at a
//! cursor it advances itself (output-clocked, so it can never drift the way
//! the free-running worker→output routing did — see the #717 revert
//! `f1131725e`). Rendering runs OFF the audio thread; allocation is fine here.
//!
//! Two full loop periods are rendered and only the SECOND is kept: reverb and
//! delay tails from cycle 1 flow into cycle 2, so the kept cycle is
//! steady-state and loops seamlessly.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use domain::io_binding::IoBinding;
use project::chain::Chain;

use crate::runtime::{build_chain_runtime_state, process_input_f32, process_output_f32};
use crate::DiPcm;

/// Frames stepped per render iteration (matches the smallest supported device
/// buffer, so block-sized DSP like the IR partitions behaves as it does live).
const RENDER_BLOCK_FRAMES: usize = 256;

/// One steady-state loop period, post-FX, ready for output-clocked playback.
pub struct DiRenderedLoop {
    /// Stereo frames at `sample_rate`, exactly one loop period long.
    pub frames: Vec<[f32; 2]>,
    pub sample_rate: u32,
}

/// Render `pcm` through a copy of `chain`'s block graph at `output_rate`,
/// draining the flat `output_index` route (so THAT output's channel layout,
/// mixdown and the chain volume/limiter path all apply, exactly as live).
pub fn render_di_loop(
    chain: &Chain,
    registry: &[IoBinding],
    output_index: usize,
    output_rate: u32,
    pcm: &DiPcm,
) -> Result<DiRenderedLoop> {
    let runtime = Arc::new(build_chain_runtime_state(
        chain,
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

    // The chosen output's route decides channel layout + mixdown, exactly as
    // the live drain path does.
    let (left, right, width) = {
        let routes = runtime.output_routes.load();
        let route = routes
            .get(output_index)
            .ok_or_else(|| anyhow!("chain has no output route {output_index}"))?;
        let left = route.output_channels.first().copied().unwrap_or(0);
        let right = route.output_channels.get(1).copied().unwrap_or(left);
        let width = route.output_channels.iter().copied().max().unwrap_or(0) + 1;
        (left, right, width)
    };

    let silence = vec![0.0f32; RENDER_BLOCK_FRAMES];
    let mut out = vec![0.0f32; RENDER_BLOCK_FRAMES * width];
    let total = loop_len * 2;
    let mut collected: Vec<[f32; 2]> = Vec::with_capacity(total + RENDER_BLOCK_FRAMES);
    while collected.len() < total {
        process_input_f32(&runtime, 0, &silence, 1);
        process_output_f32(&runtime, output_index, &mut out, width);
        for frame in out.chunks(width) {
            collected.push([frame[left], frame[right]]);
        }
    }

    Ok(DiRenderedLoop {
        frames: collected[loop_len..total].to_vec(),
        sample_rate: output_rate,
    })
}

/// Render for a CHOSEN output (`Chain.di_output`): the loop is fed through
/// the chosen output's OWN binding. #716 routes a binding's inputs only to
/// that binding's outputs and the loop substitutes segment 0 only (#699) —
/// rendering the full multi-binding chain at a second binding's output
/// drains pure silence (the owner's "no sound" on a two-interface rig). The
/// chain is reduced to the target binding for the render; the block graph is
/// unchanged, so the sound is identical.
pub fn render_di_loop_routed(
    chain: &Chain,
    registry: &[IoBinding],
    di_output: Option<&project::chain::DiOutputRef>,
    output_rate: u32,
    pcm: &DiPcm,
) -> Result<DiRenderedLoop> {
    // Which binding + which output within it is the target. `None` or a
    // stale ref → the main output (the first binding that has outputs).
    let ports: Vec<_> = crate::runtime_endpoints::resolve_chain_io_by_binding(chain, registry)
        .into_iter()
        .filter(|g| !g.outputs.is_empty())
        .collect();
    let Some((binding_id, local_index)) = di_output
        .and_then(|target| {
            let group = ports.iter().find(|g| g.binding_id == target.binding_id)?;
            let binding = registry.iter().find(|b| b.id == group.binding_id)?;
            let local = binding
                .outputs
                .iter()
                .position(|ep| ep.name == target.endpoint)?;
            Some((group.binding_id.clone(), local))
        })
        .or_else(|| ports.first().map(|g| (g.binding_id.clone(), 0)))
    else {
        // No bound outputs (e.g. a chain playing through its implicit
        // default route) — render the chain as-is on route 0, exactly the
        // pre-#771 behaviour.
        return render_di_loop(chain, registry, 0, output_rate, pcm);
    };

    // Reduce the chain's head/tail I/O to the target binding so the loop
    // (segment 0) feeds the SAME binding's output route. Blocks are
    // untouched — the DSP is identical.
    let mut reduced = chain.clone();
    reduced.io_binding_ids = vec![binding_id];
    render_di_loop(&reduced, registry, local_index, output_rate, pcm)
}
