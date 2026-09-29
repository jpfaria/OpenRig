//! Responsibility: handles the chain endpoint checklist command.
//!
//! #328 (spec §1.3, §5.3): the graph's input and output nodes list every
//! endpoint of the chain's E/S, checked by default. Unchecking one records it
//! in the chain's `disabled_endpoints` for that node only. The rig capture
//! (`sync_synthetic_into_rig`) carries the list into the chain's `RigInput`,
//! which is what `project.openrig` persists.

use anyhow::Result;

use project::endpoint_disables::EndpointRef;

use crate::command::{ChainCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_chain_endpoint_enabled(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Chain(ChainCommand::SetChainEndpointEnabled {
            chain,
            node,
            io,
            endpoint,
            enabled,
        }) = cmd
        else {
            unreachable!("handle_chain_endpoint_enabled received {cmd:?}");
        };
        self.with_chain(&chain, |c| {
            // Part 1 Task 8 owns the toggle rule (idempotent, per node).
            c.disabled_endpoints
                .set_enabled(node, EndpointRef { io, endpoint }, enabled);
            Ok(())
        })?;
        // A routing change: the MCP/MIDI drain re-syncs exactly the chain an
        // event names (`runtime_sync_policy`), never a neighbour.
        Ok(vec![Event::ChainReloaded { chain }, Event::ProjectMutated])
    }
}
