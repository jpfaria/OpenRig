//! Issue #522: per-block enable/disable fast path on the controller.
//!
//! `Command::ToggleBlockEnabled` used to go through `upsert_chain` →
//! `resolve_chain_audio_config` (CPAL device queries) → full chain rebuild.
//! For a one-bit flip on a block, the audio engine already supports
//! click-safe `FadeState` transitions on the live `BlockRuntimeNode`
//! (see `engine::runtime::set_block_enabled`). This module exposes that
//! capability at the controller boundary so the GUI can take the cheap
//! path and fall back to `upsert_chain` only when the chain has no live
//! runtime yet (e.g. first enable) or the block is a `Bypass` node that
//! needs a real processor.
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
}
