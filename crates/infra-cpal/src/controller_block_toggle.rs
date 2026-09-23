//! Responsibility: toggles a block without rebuilding the chain.
//! Issue #522: per-block AND per-chain enable/disable fast paths on the
//! controller.
//!
//! `BlockCommand::ToggleBlockEnabled` used to go through `upsert_chain` →
//! `resolve_chain_audio_config` (CPAL device queries) → full chain rebuild.
//! For a one-bit flip on a block, the audio engine already supports
//! click-safe `FadeState` transitions on the live `BlockRuntimeNode`
//! (see `engine::runtime::set_block_enabled`).
//!
//! (The per-chain pause of #522 lived here too; since #929 switching a chain
//! off kills every stream it owns — `kill_chain_streams` on the controller.)
//!
//! Lives in its own file to keep `controller.rs` within the 600-LOC cap.

use anyhow::{anyhow, Result};

use domain::ids::{BlockId, ChainId};
use project::chain::Chain;

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
        self.remember_toggle_if_building(chain_id, block_id, enabled);
        Ok(())
    }

    /// The live block-toggle path the GUI takes for `BlockCommand::ToggleBlockEnabled`:
    /// flip the block on the guitar runtime (the #522 fast path) AND re-render a
    /// monitored DI so the toggle is audible there too.
    ///
    /// The DI is a dedicated pre-render of the chain's DSP (issue #717/#771); the
    /// fast path only touches the guitar runtime, so without the re-arm a block
    /// disabled while monitoring the DI keeps sounding on the DI — the owner's
    /// "I disable a block and the effect keeps going". The re-arm is a no-op when
    /// nothing is armed and builds its routed runtime off-thread, so it does not
    /// reintroduce the freeze the fast path exists to avoid.
    pub fn toggle_block_enabled_live(
        &self,
        chain: &Chain,
        block_id: &BlockId,
        enabled: bool,
    ) -> Result<()> {
        self.set_block_enabled(&chain.id, block_id, enabled)?;
        self.rearm_di_stream_after_rebuild(chain);
        Ok(())
    }
}
