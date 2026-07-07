//! Issue #771 — an armed DI loop plays on its own isolated, PRE-RENDERED
//! stream, output-clocked, never injected into the guitar's runtime.
//!
//! Arming resolves the chain's persisted output choice (`Chain.di_output`),
//! pre-renders the loop through a fresh copy of the chain's block graph on a
//! short-lived `di-render` thread, and parks the result in the CHOSEN
//! output's [`DiPlaybackCell`]. That output device's callback mixes the
//! buffer at a cursor it advances itself — the output clock IS the DI clock,
//! so it can never drift (the free-running worker→output routing tried in
//! #717 drifted and was reverted, `f1131725e`). The guitar runtime is never
//! touched (invariant #4).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;

use domain::ids::ChainId;
use engine::di_output_resolve::resolve_di_output_index;
use engine::di_render::render_di_loop_routed;
use engine::runtime_endpoints::resolve_chain_io;
use engine::DiPcm;
use project::chain::Chain;

use crate::di_playback::{DiPlayback, DiPlaybackCell};
use crate::ProjectRuntimeController;

/// Bookkeeping for one armed DI: the output cell that holds (or, while the
/// render still runs, will hold) the playback, and the cancel flag the render
/// thread checks before parking. Dropping the handle cancels a pending render
/// and silences the cell — the render thread itself is detached (a long NAM
/// render must never block the frontend on disarm).
pub(crate) struct DiStreamHandle {
    output_index: usize,
    cell: DiPlaybackCell,
    cancel: Arc<AtomicBool>,
}

impl Drop for DiStreamHandle {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cell.store(None);
    }
}

impl ProjectRuntimeController {
    /// The playback cell for `(chain, flat output index)`, created on demand.
    /// Stream builds and arming both resolve through here, so the SAME cell is
    /// shared regardless of which side runs first, and it survives rebuilds.
    pub(crate) fn di_playback_cell(
        &self,
        chain_id: &ChainId,
        output_index: usize,
    ) -> DiPlaybackCell {
        self.di_playback_cells
            .borrow_mut()
            .entry((chain_id.clone(), output_index))
            .or_insert_with(|| Arc::new(arc_swap::ArcSwapOption::from(None)))
            .clone()
    }

    /// The rate the chain's `output_index` stream actually runs at: the live
    /// stream signature when the chain is active, else the controller's last
    /// resolved rate (never a hardcoded constant — #669/#723).
    fn di_output_rate(&self, chain_id: &ChainId, output_index: usize) -> u32 {
        self.active_chains
            .get(chain_id)
            .and_then(|active| active.stream_signature.outputs.get(output_index))
            .map(|sig| sig.sample_rate)
            .unwrap_or(self.sample_rate)
    }

    /// Arm the chain's DI: resolve the chosen output, pre-render the loop
    /// through a copy of the chain's block graph off-thread, and park the
    /// playback on that output's cell. The guitar runtime is NEVER touched.
    pub fn arm_di_stream(&self, chain: &Chain, pcm: Arc<DiPcm>) -> Result<()> {
        // Re-arm replaces any previous playback (drop clears the old cell and
        // cancels its pending render).
        self.disarm_di_stream(&chain.id);

        let output_index =
            resolve_di_output_index(chain, &self.io_bindings, chain.di_output.as_ref());
        let output_rate = self.di_output_rate(&chain.id, output_index);
        let (_, outputs) = resolve_chain_io(chain, &self.io_bindings);
        let dest = outputs
            .get(output_index)
            .map(|o| o.channels.clone())
            .unwrap_or_default();
        let dest_left = dest.first().copied().unwrap_or(0);
        let dest_right = dest.get(1).copied().unwrap_or(dest_left);

        let cell = self.di_playback_cell(&chain.id, output_index);
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let cell = cell.clone();
            let cancel = Arc::clone(&cancel);
            let chain = chain.clone();
            let registry = self.io_bindings.clone();
            std::thread::Builder::new()
                .name("di-render".into())
                .spawn(move || {
                    // Routed render (#771 owner bug): the loop must be fed
                    // through the CHOSEN output's own binding — a flat-index
                    // render on a multi-binding chain drains silence for any
                    // binding but the first (#716/#699).
                    match render_di_loop_routed(
                        &chain,
                        &registry,
                        chain.di_output.as_ref(),
                        output_rate,
                        &pcm,
                    ) {
                        Ok(rendered) => {
                            if cancel.load(Ordering::Relaxed) {
                                return;
                            }
                            let raw = Arc::new(pcm.to_loop_at(output_rate));
                            cell.store(Some(Arc::new(DiPlayback::new(
                                Arc::new(rendered),
                                raw,
                                dest_left,
                                dest_right,
                            ))));
                        }
                        Err(e) => {
                            log::error!("di-render failed for chain '{}': {e:#}", chain.id.0)
                        }
                    }
                })
                .expect("spawn di-render thread");
        }
        self.di_streams.borrow_mut().insert(
            chain.id.clone(),
            DiStreamHandle {
                output_index,
                cell,
                cancel,
            },
        );
        Ok(())
    }

    /// Disarm the chain's DI: cancel a pending render and silence the cell.
    pub fn disarm_di_stream(&self, chain_id: &ChainId) {
        self.di_streams.borrow_mut().remove(chain_id);
    }

    /// Whether the chain's DI is currently armed (the render may still be
    /// running; the playback parks when it finishes).
    pub fn di_stream_active(&self, chain_id: &ChainId) -> bool {
        self.di_streams.borrow().contains_key(chain_id)
    }

    /// Length (frames) of the parked pre-rendered loop — `None` while the
    /// render is still running or when not armed.
    pub fn di_stream_loop_len(&self, chain_id: &ChainId) -> Option<usize> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.cell.load().as_ref().map(|p| p.loop_len()))
    }

    /// #771: which flat output index currently has the chain's pre-rendered
    /// playback parked, if any. `None` while the render is still running or
    /// when the DI is not armed.
    pub fn di_playback_active_output(&self, chain_id: &ChainId) -> Option<usize> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.cell.load().is_some().then_some(h.output_index))
    }

    /// Linear `(in, out)` peaks of the DI playback's last mixed window — the
    /// DI meter row's source (the DI's OWN levels, not the chain's).
    pub fn di_playback_peaks(&self, chain_id: &ChainId) -> Option<(f32, f32)> {
        self.di_streams
            .borrow()
            .get(chain_id)
            .and_then(|h| h.cell.load().as_ref().map(|p| p.peaks()))
    }
}
