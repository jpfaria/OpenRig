//! Responsibility: commits the chain the editor was drafting.
//!
//! Split out of `chain_save_cancel_callbacks` and
//! `chain_editor_save_cancel_callbacks` (#913), which carried a copy each —
//! the AppWindow's inline editor and the detached `ChainEditorWindow` save the
//! same way and must not drift.
//!
//! #716: the saved chain carries its effect blocks plus the SELECTED BINDING
//! IDS, never materialized I/O blocks — the engine resolves endpoints from the
//! registry when it builds streams, and embedding them here put per-endpoint
//! input tiles in the strip. Which is also why an empty selection is refused:
//! a chain with no binding has nothing to open.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command};
use domain::ids::ChainId;
use domain::AudioDeviceDescriptor;
use slint::VecModel;

use crate::chain_editor::chain_from_draft;
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::{ChainDraft, ProjectSession};
use crate::ProjectChainItem;

/// Why the save did not happen. Each maps to a message the editor shows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SaveChainError {
    /// No binding selected in the checklist.
    NoBindingSelected,
    /// The dispatcher or the runtime resync refused; carries the message.
    Failed(String),
}

/// Save `draft` — replacing the chain at its `editing_index`, or appending a
/// new one — and republish the chain rows.
///
/// Returns the saved chain's id so the caller can refresh what is keyed on it.
pub(crate) fn save_drafted_chain(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    draft: &ChainDraft,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<ChainId, SaveChainError> {
    if draft.io_binding_ids.is_empty() {
        return Err(SaveChainError::NoBindingSelected);
    }
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        return Err(SaveChainError::Failed("no project loaded".to_string()));
    };
    let existing = draft
        .editing_index
        .and_then(|index| session.project.borrow().chains.get(index).cloned());
    let chain = chain_from_draft(draft, existing.as_ref());
    let chain_id = chain.id.clone();
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChain { chain }))
        .map_err(|e| SaveChainError::Failed(e.to_string()))?;
    // #614: the command records the edit; the live chain only plays it once
    // its runtime is rebuilt.
    request_chain_sync(session, &chain_id).map_err(|e| SaveChainError::Failed(e.to_string()))?;
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    Ok(chain_id)
}

#[cfg(test)]
#[path = "chain_draft_save_tests.rs"]
mod tests;
