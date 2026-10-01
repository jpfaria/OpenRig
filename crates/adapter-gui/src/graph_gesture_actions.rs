//! Responsibility: turns a chain graph gesture into the command it asks for.
//!
//! #328 (spec §3, §5.1). A gesture names a node by id and becomes one
//! command on the bus (`BlockCommand`, or `SplitCommand::RemoveSplit`). A
//! structural change is then resynced into the live chain (#614: a dispatch
//! alone only records it) and the rows are republished. A drop that does not
//! apply republishes too: the card followed the pointer in place and has to
//! go back to its slot.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, Command, SplitCommand};
use domain::ids::BlockId;
use domain::AudioDeviceDescriptor;
use project::chain::Chain;
use slint::VecModel;

use crate::chain_block_lists::split_by_id;
use crate::chain_graph_ids::{resolve_node, NodeRef};
use crate::graph_anchor::{move_target, parse_anchor};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// Where the rows get republished.
pub(crate) struct RowsTarget<'a> {
    pub(crate) model: &'a Rc<VecModel<ProjectChainItem>>,
    pub(crate) inputs: &'a [AudioDeviceDescriptor],
    pub(crate) outputs: &'a [AudioDeviceDescriptor],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum GestureError {
    NoProject,
    NoSuchChain,
    /// The node cannot take this gesture (an I/O node has nothing to bypass).
    NotApplicable,
    /// The drop changes nothing.
    NoMove,
    /// The dispatcher or the runtime sync refused; carries the message.
    Failed(String),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RemoveOutcome {
    Removed,
    /// Path B holds blocks and removing the split deletes them (spec §3):
    /// ask first. `name` is what the confirm dialog shows, `split` the split
    /// the confirmation removes.
    ConfirmSplit {
        name: String,
        split: BlockId,
    },
}

type Session = Rc<RefCell<Option<ProjectSession>>>;

fn failed(error: impl std::fmt::Display) -> GestureError {
    GestureError::Failed(error.to_string())
}

fn chain_of(session: &ProjectSession, chain_index: usize) -> Result<Chain, GestureError> {
    session
        .project
        .borrow()
        .chains
        .get(chain_index)
        .cloned()
        .ok_or(GestureError::NoSuchChain)
}

fn republish(session: &ProjectSession, rows: &RowsTarget<'_>) {
    replace_project_chains(
        rows.model,
        &session.project.borrow(),
        rows.inputs,
        rows.outputs,
        &session.io_bindings.borrow(),
    );
}

/// Dispatch a structural change, resync the chain's runtime, republish.
fn apply(
    session: &ProjectSession,
    chain: &Chain,
    command: Command,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    session.dispatcher.dispatch(command).map_err(failed)?;
    request_chain_sync(session, &chain.id).map_err(failed)?;
    republish(session, rows);
    Ok(())
}

pub(crate) fn toggle_node(
    session: &Session,
    chain_index: usize,
    node_id: &str,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = chain_of(session, chain_index)?;
    let Some(NodeRef::Block { id, .. }) = resolve_node(&chain, node_id) else {
        return Err(GestureError::NotApplicable);
    };
    // #127: the dispatcher applies the live toggle itself (`block_toggle.rs`).
    session
        .dispatcher
        .dispatch(Command::Block(BlockCommand::ToggleBlockEnabled {
            chain: chain.id.clone(),
            block: id,
        }))
        .map_err(failed)?;
    republish(session, rows);
    Ok(())
}

pub(crate) fn remove_node(
    session: &Session,
    chain_index: usize,
    node_id: &str,
    rows: &RowsTarget<'_>,
) -> Result<RemoveOutcome, GestureError> {
    let split_id = {
        let borrowed = session.borrow();
        let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
        let chain = chain_of(s, chain_index)?;
        match resolve_node(&chain, node_id).ok_or(GestureError::NotApplicable)? {
            NodeRef::Block { id, .. } => {
                let command = Command::Block(BlockCommand::RemoveBlock {
                    chain: chain.id.clone(),
                    block: id,
                });
                apply(s, &chain, command, rows)?;
                return Ok(RemoveOutcome::Removed);
            }
            NodeRef::Split { id } | NodeRef::Mixer { id } => {
                let (_, split) = split_by_id(&chain, &id).ok_or(GestureError::NotApplicable)?;
                let dropped: usize = split.paths.iter().skip(1).map(Vec::len).sum();
                if dropped > 0 {
                    let name = rust_i18n::t!("confirm-remove-split-name", n = dropped);
                    return Ok(RemoveOutcome::ConfirmSplit {
                        name: name.to_string(),
                        split: id,
                    });
                }
                id
            }
            NodeRef::Endpoints(_) => return Err(GestureError::NotApplicable),
        }
    };
    remove_split(session, chain_index, &split_id, rows)?;
    Ok(RemoveOutcome::Removed)
}

/// `RemoveSplit` of the split `split_id`: path A's blocks take its place,
/// the other paths' go (spec §3, §11).
pub(crate) fn remove_split(
    session: &Session,
    chain_index: usize,
    split_id: &BlockId,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = chain_of(s, chain_index)?;
    split_by_id(&chain, split_id).ok_or(GestureError::NotApplicable)?;
    let command = Command::Split(SplitCommand::RemoveSplit {
        chain: chain.id.clone(),
        split_id: split_id.clone(),
    });
    apply(s, &chain, command, rows)
}

pub(crate) fn drop_node(
    session: &Session,
    chain_index: usize,
    node_id: &str,
    anchor: &str,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let s = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain = chain_of(s, chain_index)?;
    let result = match parse_anchor(anchor).and_then(|slot| move_target(&chain, node_id, &slot)) {
        None => Err(GestureError::NoMove),
        Some(target) => {
            let command = Command::Block(BlockCommand::MoveBlock {
                chain: chain.id.clone(),
                block: target.block,
                new_position: target.new_position,
                path: target.path,
            });
            apply(s, &chain, command, rows)
        }
    };
    if result.is_err() {
        // The card followed the pointer in place: put it back in its slot.
        republish(s, rows);
    }
    result
}

#[cfg(test)]
#[path = "graph_gesture_actions_tests.rs"]
mod tests;
