//! #614 — `Command::SetChainDiLoopSource` / `SetChainDiLoopEnabled`:
//! per-chain virtual DI loop (file-per-feature; #436 dispatcher split).
//!
//! **EPHEMERAL — never serialized into the project** (a project-level DI
//! persistence is tracked separately in #324).
//!
//! ## Design
//! - `SetChainDiLoopSource { chain, source }` — loads the DI loop off the
//!   audio thread via `load_di_loop`, stores `(source, Arc<DiLoop>)` in the
//!   dispatcher's `di_loop_state` map, and emits
//!   `Event::ChainDiLoopSourceChanged { chain }`. Returns `Err` on decode
//!   failure (never swallowed silently) and on missing chain.
//!
//! - `SetChainDiLoopEnabled { chain, enabled }` — arms (or disarms) THIS
//!   chain's isolated DI stream through the attached
//!   [`crate::runtime_control::RuntimeControl`], then emits
//!   `Event::ChainDiLoopEnabledChanged { chain, enabled }`. Returns `Err` on
//!   missing chain, and on a runtime that could not arm.
//!
//! - `SetChainDiLoopOutput { chain, output }` — persists the chosen endpoint
//!   and, when the loop is already sounding, asks the runtime to follow it.
//!
//! #127: the runtime effect is applied HERE, by the dispatcher. It used to be
//! applied by the GUI callback that had just dispatched, which meant the same
//! command over MCP/gRPC/MIDI emitted its event and played nothing.

use anyhow::Result;

use crate::command::{ChainCommand, Command};
use crate::di_loader::load_di_loop;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// This chain as the runtime needs to see it — the whole (live) chain,
    /// because arming resolves its `di_output` and renders through a copy of
    /// its blocks. `None` when the chain is gone.
    ///
    /// A clone, deliberately: the borrow ends with the call, so the frontend's
    /// `RuntimeControl` is free to read the project back while it arms.
    pub(crate) fn chain_def(&self, chain: &domain::ids::ChainId) -> Option<project::chain::Chain> {
        self.project
            .borrow()
            .chains
            .iter()
            .find(|c| &c.id == chain)
            .cloned()
    }

    /// The decoded (still un-resampled) DI source stored for this chain, if
    /// one has been loaded. Resampling is the arm's job, per output rate.
    pub(crate) fn di_pcm(
        &self,
        chain: &domain::ids::ChainId,
    ) -> Option<std::sync::Arc<engine::DiPcm>> {
        self.di_loop_state
            .borrow()
            .get(chain)
            .map(|(_, pcm)| std::sync::Arc::clone(pcm))
    }

    /// Handle `SetChainDiLoopSource` and `SetChainDiLoopEnabled`.
    pub(crate) fn handle_di_loop(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Chain(ChainCommand::SetChainDiLoopSource { chain, source }) => {
                // Verify the chain exists before doing expensive I/O.
                {
                    let proj = self.project.borrow();
                    if proj.chains.iter().all(|c| c.id != chain) {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    }
                }

                // Cheap sync validation (one stat): a missing file still
                // errors immediately — MCP/GUI callers keep the Err
                // contract. Only the decode is deferred.
                let path =
                    crate::di_loader::resolve_path(&source).map_err(|e| anyhow::anyhow!("{e}"))?;
                if !path.exists() {
                    return Err(anyhow::anyhow!("DI loop file not found: {path:?}"));
                }

                // #693: the WAV decode runs on its own task — the
                // dispatching thread returns immediately. The completion
                // lands in `poll_async_results` (frontend tick), which
                // installs the source into `di_loop_state` and emits
                // `ChainDiLoopSourceChanged`. #749: decode only — the
                // resample happens at arm time, per output-stream rate.
                let tx = self.async_done_tx.clone();
                std::thread::Builder::new()
                    .name("di-load".into())
                    .spawn(move || {
                        let result = load_di_loop(&source);
                        let _ = tx.send(crate::local_dispatcher::AsyncDone::DiLoad(
                            chain, source, result,
                        ));
                    })
                    .map_err(|e| anyhow::anyhow!("failed to spawn di-load task: {e}"))?;

                Ok(vec![])
            }

            Command::Chain(ChainCommand::SetChainDiLoopEnabled { chain, enabled }) => {
                // Verify the chain exists, and take the snapshot the arm needs
                // in the SAME borrow — which ends here, before the runtime is
                // touched: the frontend's control reads the project back.
                let Some(chain_def) = self.chain_def(&chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };

                // #127: apply the runtime effect HERE. The GUI used to arm the
                // stream in its own callback right after dispatching, so the
                // same command from MCP/gRPC/MIDI emitted the event and played
                // nothing.
                if let Some(control) = self.runtime_control() {
                    match (enabled, self.di_pcm(&chain)) {
                        // A play with no decoded source has nothing to sound:
                        // clear the stream instead of arming silence. The GUI
                        // only offers Play once a source is loaded; MCP can ask
                        // at any time.
                        (true, Some(pcm)) => control.arm_di_stream(&chain_def, pcm)?,
                        _ => control.disarm_di_stream(&chain),
                    }
                }

                Ok(vec![Event::ChainDiLoopEnabledChanged { chain, enabled }])
            }

            Command::Chain(ChainCommand::SetChainDiLoopOutput { chain, output }) => {
                // Locate the chain and persist di_output.
                {
                    let mut proj = self.project.borrow_mut();
                    let found = proj.chains.iter_mut().find(|c| c.id == chain);
                    match found {
                        Some(c) => c.di_output = Some(output),
                        None => {
                            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                        }
                    }
                }

                // #771: a loop that is PLAYING moves to the endpoint the user
                // just picked — the refresh re-resolves the (now updated)
                // di_output and re-renders onto its cell. A stopped loop stays
                // stopped: an output pick is not a play.
                if let (Some(control), Some(chain_def), Some(pcm)) = (
                    self.runtime_control(),
                    self.chain_def(&chain),
                    self.di_pcm(&chain),
                ) {
                    control.refresh_di_stream(&chain_def, pcm)?;
                }

                Ok(vec![Event::ChainDiLoopOutputChanged { chain }])
            }

            other => {
                unreachable!("handle_di_loop received non-di-loop command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_di_loop_tests.rs"]
mod tests;
