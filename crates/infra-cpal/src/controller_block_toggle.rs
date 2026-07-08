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
        Ok(())
    }

    /// The live block-toggle path the GUI takes for `Command::ToggleBlockEnabled`:
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

    /// Pause a chain without dropping its runtime: `set_draining()` makes
    /// every audio callback short-circuit to silence, but the CPAL
    /// streams stay open and the `Arc<ChainRuntimeState>` stays in
    /// `runtime_graph` so the next enable resumes in O(1) via
    /// `upsert_chain`'s fast-path branch. No-op if the chain has no
    /// live runtime yet.
    ///
    /// Issue #545 — a chain with multiple input groups (one runtime per
    /// physical input device, see #350 Phase 3) needs every group
    /// drained. The previous implementation called
    /// `runtime_for_chain`, which is documented as "returns the first
    /// runtime" and left the other groups processing. That kept the
    /// stream taps publishing and the audio thread spending CPU, which
    /// the user observes as the chain looking alive after toggling
    /// off. Fan over `runtimes_for` so every group flips.
    pub fn pause_chain(&self, chain_id: &ChainId) {
        let runtimes = self.runtime_graph.runtimes_for(chain_id);
        if runtimes.is_empty() {
            return;
        }
        log::info!(
            "pausing chain '{}' across {} input group(s) (keep streams alive)",
            chain_id.0,
            runtimes.len(),
        );
        for runtime in &runtimes {
            runtime.set_draining();
        }
    }
}
