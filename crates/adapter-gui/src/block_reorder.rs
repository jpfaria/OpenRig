//! Responsibility: moves a block to the slot it was dragged before.
//!
//! Split out of `chain_block_crud_wiring` (#913). The drag reports "put the
//! block at FROM before the one at BEFORE", and the position that has to be
//! dispatched is the one AFTER the block is lifted out — off by one whenever it
//! moves rightwards. Getting that wrong drops the block one slot short of where
//! the user let go, which in a signal chain is an audible difference.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, Command};
use domain::ids::ChainId;
use domain::AudioDeviceDescriptor;
use slint::VecModel;

use crate::chain_block_helpers::ui_index_to_real_block_index;
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// Why the move did not happen.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReorderBlockError {
    NoProject,
    NoSuchChain,
    /// The drop changes nothing — out of range, or onto its own position.
    NoMove,
    /// The dispatcher or the runtime resync refused; carries the message.
    Failed(String),
}

/// The insert position for a block lifted from `from` and dropped before
/// `before`, in a chain of `count` blocks.
///
/// `None` ⇒ the drop is a no-op: out of range, onto itself, or into the gap it
/// already occupies (`before == from + 1` is the slot immediately after it).
fn insert_position(from: usize, before: usize, count: usize) -> Option<usize> {
    if from >= count {
        return None;
    }
    if before == from || before == from + 1 {
        return None;
    }
    // The block is REMOVED before it is inserted, so everything to its right
    // shifts one slot left.
    let normalized = if before > from { before - 1 } else { before };
    Some(normalized.min(count.saturating_sub(1)))
}

/// Move the block the row at `ui_from` shows to before the row at `ui_before`.
pub(crate) fn reorder_block(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    ui_from: usize,
    ui_before: usize,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<ChainId, ReorderBlockError> {
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        return Err(ReorderBlockError::NoProject);
    };
    let (chain_id, block_id, insert_at) = {
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index) else {
            return Err(ReorderBlockError::NoSuchChain);
        };
        let from = ui_index_to_real_block_index(chain, ui_from);
        let before = ui_index_to_real_block_index(chain, ui_before);
        let Some(insert_at) = insert_position(from, before, chain.blocks.len()) else {
            return Err(ReorderBlockError::NoMove);
        };
        (chain.id.clone(), chain.blocks[from].id.clone(), insert_at)
    };
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::MoveBlock {
            chain: chain_id.clone(),
            block: block_id,
            new_position: insert_at,
        }))
        .map_err(|e| ReorderBlockError::Failed(e.to_string()))?;
    request_chain_sync(session, &chain_id).map_err(|e| ReorderBlockError::Failed(e.to_string()))?;
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
#[path = "block_reorder_tests.rs"]
mod tests;
