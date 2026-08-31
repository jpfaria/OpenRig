//! Responsibility: flips one block's enabled state from its row.
//!
//! Split out of `chain_block_crud_wiring` (#913). The row index the user
//! touched goes through `ui_index_to_real_block_index` rather than straight
//! into `chain.blocks`: under model A (#716) that mapping is the identity, but
//! it is the one place that decides, and reading the index directly here would
//! silently diverge from the strip the day it stops being.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, Command};
use domain::AudioDeviceDescriptor;
use slint::VecModel;

use crate::chain_block_helpers::ui_index_to_real_block_index;
use crate::project_view::replace_project_chains;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// What the toggle changed, so the caller can mirror it onto the drawer.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ToggledBlock {
    /// The block's position in `chain.blocks` that the row resolved to.
    pub(crate) block_index: usize,
    /// Its state after the toggle.
    pub(crate) enabled: bool,
}

/// Why the toggle did not happen. Each maps to a message the caller shows.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ToggleBlockError {
    NoProject,
    NoSuchChain,
    NoSuchBlock,
    Failed(String),
}

/// Toggle the block the row at `ui_block_index` shows, then republish the rows.
pub(crate) fn toggle_block_at_row(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    ui_block_index: usize,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<ToggledBlock, ToggleBlockError> {
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        return Err(ToggleBlockError::NoProject);
    };
    let (block_index, chain_id, block_id) = {
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index) else {
            return Err(ToggleBlockError::NoSuchChain);
        };
        let block_index = ui_index_to_real_block_index(chain, ui_block_index);
        let Some(block) = chain.blocks.get(block_index) else {
            return Err(ToggleBlockError::NoSuchBlock);
        };
        (block_index, chain.id.clone(), block.id.clone())
    };
    // #127: the dispatcher applies the live toggle through `RuntimeControl`,
    // so a runtime failure comes back out of this dispatch.
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: chain_id,
            block: block_id,
        }))
        .map_err(|e| ToggleBlockError::Failed(e.to_string()))?;
    let enabled = session
        .project
        .borrow()
        .chains
        .get(chain_index)
        .and_then(|chain| chain.blocks.get(block_index))
        .map(|block| block.enabled)
        .unwrap_or(false);
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    Ok(ToggledBlock {
        block_index,
        enabled,
    })
}

#[cfg(test)]
#[path = "block_toggle_tests.rs"]
mod tests;
