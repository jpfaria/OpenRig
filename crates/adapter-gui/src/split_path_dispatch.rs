//! Responsibility: runs one split-path command on a chain of the open project.
//!
//! #328 (spec §11): "+ path" and remove-path go on the bus like the Mix / Y
//! switch (`split_end_switch`): the command owns the rules, a refusal comes
//! back as `GestureError::Failed` for the toast, and an accepted edit is
//! resynced into the live chain (#614) before the rows are republished.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::Command;
use project::chain::Chain;

use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;

/// Dispatch the command `build` makes from chain `chain_index`; `None` from
/// `build` means the gesture does not apply to that chain.
pub(crate) fn dispatch_split_path(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    build: impl FnOnce(&Chain) -> Option<Command>,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let (chain_id, command) = {
        let project = session.project.borrow();
        let chain = project
            .chains
            .get(chain_index)
            .ok_or(GestureError::NoSuchChain)?;
        let command = build(chain).ok_or(GestureError::NotApplicable)?;
        (chain.id.clone(), command)
    };
    session
        .dispatcher
        .dispatch(command)
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
