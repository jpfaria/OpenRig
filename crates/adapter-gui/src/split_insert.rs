//! Responsibility: adds a split to a chain from the add-block picker.
//!
//! #328 (spec §3, §5.1, §10). `AddSplit` creates an empty split with default
//! knobs at the picked position of the picked list (top level or a path); the live chain is resynced (#614) and the rows are
//! republished so the new lanes appear.

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{Command, SplitCommand};
use project::block::{PathRef, SplitEnd};

use crate::graph_gesture_actions::{GestureError, RowsTarget};
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::split_picker_entries::{split_end_for_pick, split_picker_ends};
use crate::state::{BlockEditorDraft, ProjectSession};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SplitPick {
    pub(crate) chain_index: usize,
    pub(crate) position: usize,
    pub(crate) end: SplitEnd,
    pub(crate) path: Option<PathRef>,
}

/// The split the picker row at `index` stands for, if it is a split entry.
pub(crate) fn split_pick(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    draft: &BlockEditorDraft,
    index: usize,
    base_len: usize,
) -> Option<SplitPick> {
    let borrowed = session.borrow();
    let project = borrowed.as_ref()?.project.borrow();
    let chain = project.chains.get(draft.chain_index)?;
    let ends = split_picker_ends(chain, draft.before_index, draft.path.as_ref());
    Some(SplitPick {
        chain_index: draft.chain_index,
        position: draft.before_index,
        end: split_end_for_pick(index, base_len, &ends)?,
        path: draft.path.clone(),
    })
}

pub(crate) fn add_split(
    session: &Rc<RefCell<Option<ProjectSession>>>,
    pick: &SplitPick,
    rows: &RowsTarget<'_>,
) -> Result<(), GestureError> {
    let borrowed = session.borrow();
    let session = borrowed.as_ref().ok_or(GestureError::NoProject)?;
    let chain_id = session
        .project
        .borrow()
        .chains
        .get(pick.chain_index)
        .map(|c| c.id.clone())
        .ok_or(GestureError::NoSuchChain)?;
    session
        .dispatcher
        .dispatch(Command::Split(SplitCommand::AddSplit {
            chain: chain_id.clone(),
            position: pick.position,
            end: pick.end,
            path: pick.path.clone(),
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
