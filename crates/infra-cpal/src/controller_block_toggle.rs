//! Issue #522: per-block AND per-chain enable/disable fast paths on the
//! controller.
//!
//! `Command::ToggleBlockEnabled` used to go through `upsert_chain` →
//! `resolve_chain_audio_config` (CPAL device queries) → full chain rebuild.
//! For a one-bit flip on a block, the audio engine already supports
//! click-safe `FadeState` transitions on the live `BlockRuntimeNode`
//! (see `engine::runtime::set_block_enabled`).
//!
//! `Command::ToggleChainEnabled` used to drop the runtime entirely on
//! disable and rebuild from scratch on the next enable. `pause_chain`
//! keeps the runtime alive and just flips `set_draining()`; resume is
//! the matching `clear_draining()` — both O(1), no NAM reload, no CPAL
//! touch.
//!
//! Lives in its own file to keep `controller.rs` within the 600-LOC cap.

use anyhow::{anyhow, Result};

use domain::ids::{BlockId, ChainId};

use crate::ProjectRuntimeController;

impl ProjectRuntimeController {
    /// Flip the block's enabled state in place on every per-input runtime
    /// of the chain, with no CPAL re-resolve and no processor rebuild.
    /// Returns `Err` if the chain has no live runtime OR if any runtime
    /// requires a full rebuild (caller falls back to `upsert_chain`).
    pub fn set_block_enabled(
        &self,
        chain_id: &ChainId,
        block_id: &BlockId,
        enabled: bool,
    ) -> Result<()> {
        let runtimes = self.runtime_graph.runtimes_for(chain_id);
        if runtimes.is_empty() {
            return Err(anyhow!(
                "chain '{}' has no live runtime — needs full rebuild",
                chain_id.0
            ));
        }
        for runtime in &runtimes {
            engine::runtime::set_block_enabled(runtime.as_ref(), block_id, enabled)?;
        }
        Ok(())
    }

    /// Pause a chain without dropping its runtime: `set_draining()` makes
    /// every audio callback short-circuit to silence, but the CPAL
    /// streams stay open and the `Arc<ChainRuntimeState>` stays in
    /// `runtime_graph` so the next enable resumes in O(1) via
    /// `upsert_chain`'s fast-path branch. No-op if the chain has no
    /// live runtime yet.
    pub fn pause_chain(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            log::info!("pausing chain '{}' (keep streams alive)", chain_id.0);
            runtime.set_draining();
        }
    }
}
