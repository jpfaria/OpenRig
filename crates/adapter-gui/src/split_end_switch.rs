//! Responsibility: switches a chain's split between Split → Mix and Y → A/B.
//!
//! #328 (orchestrator decision 9): the split editor's Mix / Y switch is a
//! `SetSplitEnd` on the bus. The command owns the rule (no Y while a
//! processing block follows the split) and its refusal comes back as
//! `GestureError::Failed` for the toast. An accepted switch is resynced into
//! the live chain (#614) and the rows are republished.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SplitCommand};
use project::block::SplitEnd;

use crate::chain_block_lists::split_of;
use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;

pub(crate) fn set_split_end(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    end: SplitEnd,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let (chain_id, split_id) = {
        let project = session.project.borrow();
        let chain = project
            .chains
            .get(chain_index)
            .ok_or(GestureError::NoSuchChain)?;
        let (_, split_id, _) = split_of(chain).ok_or(GestureError::NotApplicable)?;
        (chain.id.clone(), split_id.clone())
    };
    session
        .dispatcher
        .dispatch(Command::Split(SplitCommand::SetSplitEnd {
            chain: chain_id.clone(),
            split_id,
            end,
        }))
        .map_err(|e| GestureError::Failed(e.to_string()))?;
    request_chain_sync(session, &chain_id).map_err(|e| GestureError::Failed(e.to_string()))?;
    replace_project_chains(
        rows.model,
        &session.project.borrow(),
        rows.inputs,
        rows.outputs,
        &session.io_bindings.borrow(),
    );
    Ok(())
}

#[cfg(test)]
#[path = "split_end_switch_tests.rs"]
mod tests;
