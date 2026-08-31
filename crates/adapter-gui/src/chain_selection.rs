//! Responsibility: makes the tapped chain the active one.
//!
//! Split out of `select_chain_callback` (#913). Showing the error toast and
//! redrawing the markers is screen work; deciding WHICH chain the tap means and
//! putting that on the bus is not — the footswitch reads
//! `SelectionState.active_chain`, so a tap that never reaches the dispatcher
//! leaves the pedal acting on whatever chain was selected last (#591).

use domain::ids::ChainId;

use crate::state::ProjectSession;
use application::command::{Command, SelectionCommand};

/// Why a tap could not select a chain. Both cases are shown to the user, so
/// the caller needs to tell them apart.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SelectChainError {
    /// The tapped index is not a chain of the open project.
    NoSuchChain,
    /// The dispatcher refused the selection.
    Rejected,
}

/// Select the chain at `index` as the active one, returning its id so the
/// caller can redraw. The index comes straight from the row the user touched.
pub(crate) fn select_chain(
    session: &ProjectSession,
    index: usize,
) -> Result<ChainId, SelectChainError> {
    let chain_id = session
        .project
        .borrow()
        .chains
        .get(index)
        .map(|c| c.id.clone())
        .ok_or(SelectChainError::NoSuchChain)?;
    session
        .dispatcher
        .dispatch(Command::Selection(SelectionCommand::SelectActiveChain {
            chain: chain_id.clone(),
        }))
        .map_err(|_| SelectChainError::Rejected)?;
    Ok(chain_id)
}

#[cfg(test)]
#[path = "chain_selection_tests.rs"]
mod tests;
