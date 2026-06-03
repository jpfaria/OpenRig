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
//! - `SetChainDiLoopEnabled { chain, enabled }` — emits
//!   `Event::ChainDiLoopEnabledChanged { chain, enabled }`. The adapter-gui
//!   wiring (Task 6) reacts: `enabled: true` → calls
//!   `runtime.set_di_loop(Some(arc))` (arc retrieved via
//!   `LocalDispatcher::di_loop_for_chain`); `enabled: false` →
//!   `runtime.set_di_loop(None)`. Returns `Err` on missing chain.
//!
//! Mirrors the `SetOutputMuted` precedent: dispatcher records intent + emits
//! event; adapter-gui applies the change to the audio runtime.

use anyhow::Result;
use std::sync::Arc;

use crate::command::Command;
use crate::di_loader::load_di_loop;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Handle `SetChainDiLoopSource` and `SetChainDiLoopEnabled`.
    pub(crate) fn handle_di_loop(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetChainDiLoopSource { chain, source } => {
                // Verify the chain exists before doing expensive I/O.
                {
                    let proj = self.project.borrow();
                    if proj.chains.iter().all(|c| c.id != chain) {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    }
                }

                let engine_sr = *self.engine_sr.borrow();
                let arc = load_di_loop(&source, engine_sr)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;

                // Store in ephemeral map — not written to the project.
                self.di_loop_state
                    .borrow_mut()
                    .insert(chain.clone(), (source, Arc::clone(&arc)));

                Ok(vec![Event::ChainDiLoopSourceChanged { chain }])
            }

            Command::SetChainDiLoopEnabled { chain, enabled } => {
                // Verify the chain exists.
                {
                    let proj = self.project.borrow();
                    if proj.chains.iter().all(|c| c.id != chain) {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    }
                }

                Ok(vec![Event::ChainDiLoopEnabledChanged { chain, enabled }])
            }

            other => {
                unreachable!("handle_di_loop received non-di-loop command: {other:?}")
            }
        }
    }
}
