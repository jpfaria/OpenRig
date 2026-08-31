//! Responsibility: removes the block the editor is pointed at.
//!
//! Split out of `block_delete_wiring` (#913). Hiding the dialog and clearing
//! the editor is screen work; resolving WHICH block the draft's indexes mean,
//! removing it on the bus and resyncing the live chain is not — a delete that
//! stopped at the project would leave the removed block still processing audio
//! until the next rebuild.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, Command};
use domain::AudioDeviceDescriptor;
use slint::VecModel;

use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::ProjectChainItem;

/// Why a delete could not happen. Every case is logged; only `Failed` is shown
/// to the user, because the others mean the dialog outlived what it pointed at.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DeleteBlockError {
    /// The draft is for a block being ADDED — there is nothing to remove yet.
    NotAnExistingBlock,
    /// No project open, or the draft's indexes no longer resolve.
    Gone,
    /// The dispatcher or the runtime sync refused; carries the message.
    Failed(String),
}

/// Remove the drafted block and republish the chain rows.
pub(crate) fn delete_drafted_block(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    draft: &BlockEditorDraft,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<(), DeleteBlockError> {
    let Some(block_index) = draft.block_index else {
        return Err(DeleteBlockError::NotAnExistingBlock);
    };
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        log::warn!("[block-drawer.delete] no project loaded");
        return Err(DeleteBlockError::Gone);
    };
    let (chain_id, block_id) = {
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(draft.chain_index) else {
            log::warn!("[block-drawer.delete] chain {} is gone", draft.chain_index);
            return Err(DeleteBlockError::Gone);
        };
        let Some(block) = chain.blocks.get(block_index) else {
            log::warn!("[block-drawer.delete] block {block_index} is gone");
            return Err(DeleteBlockError::Gone);
        };
        (chain.id.clone(), block.id.clone())
    };
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::RemoveBlock {
            chain: chain_id.clone(),
            block: block_id,
        }))
        .map_err(|e| DeleteBlockError::Failed(e.to_string()))?;
    // #614: the command records the intent; the live chain only stops playing
    // the removed block once its runtime is rebuilt.
    request_chain_sync(session, &chain_id).map_err(|e| DeleteBlockError::Failed(e.to_string()))?;
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    Ok(())
}

#[cfg(test)]
#[path = "block_delete_tests.rs"]
mod tests;
