//! #127: the chain-runtime sync handler.
//!
//! `ChainCommand::SyncChainRuntime` is the door a caller knocks on to make one
//! chain's live audio runtime catch up with the project. The sequence itself
//! (device resolve, off-thread rebuild, activation scheduling) belongs to the
//! frontend that OWNS the runtime and stays there; what moved here is WHO asks
//! for it. A UI callback used to reach into the audio controller directly,
//! which is why the same request over MCP/gRPC changed nothing.

use anyhow::Result;

use domain::ids::ChainId;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `ChainCommand::SyncChainRuntime` — re-sync the named chain's runtime
    /// through the attached [`crate::runtime_control::RuntimeControl`].
    ///
    /// Emits nothing: it changes no project state, it asks the runtime to
    /// catch up with state another command already wrote. Emitting a
    /// chain-scoped event here would make the frontend's event drain schedule
    /// a SECOND sync for the same chain — a redundant rebuild on the hot path
    /// of every block edit (#740).
    pub(crate) fn handle_sync_chain_runtime(&self, chain: ChainId) -> Result<Vec<Event>> {
        if let Some(control) = self.runtime_control() {
            control.sync_chain(&chain)?;
        }
        Ok(vec![])
    }
}

#[cfg(test)]
#[path = "local_dispatcher_runtime_sync_tests.rs"]
mod tests;
