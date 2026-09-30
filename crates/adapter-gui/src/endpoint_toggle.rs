//! Responsibility: switches one endpoint of a graph node on or off.
//!
//! #328 (spec §5.3). The row index is the checklist's (`endpoint_rows`); the
//! command names the endpoint by E/S id and endpoint name, so a reordered
//! registry never flips the wrong one. Unchecking every endpoint of a node is
//! allowed — that node's segments are simply not built. The live chain is
//! resynced (#614): which segments exist just changed.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command};

use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::endpoint_checklist_items::endpoint_rows;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;

pub(crate) fn set_endpoint_enabled(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    node_id: &str,
    row: usize,
    enabled: bool,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = s
        .project
        .borrow()
        .chains
        .get(chain_index)
        .cloned()
        .ok_or(GestureError::NoSuchChain)?;
    let Some(NodeRef::Endpoints(node)) = resolve_node(&chain, node_id) else {
        return Err(GestureError::NotApplicable);
    };
    let endpoint = endpoint_rows(&chain, &s.io_bindings.borrow(), node)
        .into_iter()
        .nth(row)
        .ok_or(GestureError::NotApplicable)?;
    s.dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainEndpointEnabled {
            chain: chain.id.clone(),
            node,
            io: endpoint.io,
            endpoint: endpoint.endpoint,
            enabled,
        }))
        .map_err(|e| GestureError::Failed(e.to_string()))?;
    request_chain_sync(s, &chain.id).map_err(|e| GestureError::Failed(e.to_string()))?;
    replace_project_chains(
        rows.model,
        &s.project.borrow(),
        rows.inputs,
        rows.outputs,
        &s.io_bindings.borrow(),
    );
    Ok(())
}

#[cfg(test)]
#[path = "endpoint_toggle_tests.rs"]
mod tests;
