//! Responsibility: applies one edited block parameter to the project.
//!
//! Split out of `block_parameter_wiring` (#913), which repeated the same
//! sequence for number, text and bool. Painting the row is screen work; what
//! this owns is the part that must be identical for all three: a parameter is
//! only committed for a block that ALREADY EXISTS (a draft still being added
//! has nothing to address), the edit goes on the bus so every transport sees
//! it, and the live chain is resynced — a dispatch alone records the value
//! without the runtime playing it (#614).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{BlockCommand, Command};
use application::event::Event;
use domain::AudioDeviceDescriptor;
use slint::VecModel;

use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::{BlockEditorDraft, ProjectSession};
use crate::ProjectChainItem;

/// The value a parameter row produced.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ParamValue {
    Number(f64),
    Text(String),
    Bool(bool),
    /// A pick from a list. Both halves travel: the string is what the project
    /// stores, the index is what the widget shows, and a command carrying only
    /// one of them leaves the other surface out of step.
    Option {
        value: String,
        index: usize,
    },
}

/// Why the edit was not committed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ApplyParamError {
    /// Nothing to address: no draft, a draft for a block being ADDED, no
    /// project, or indexes that no longer resolve. The row still shows the
    /// value — the block simply is not in the project yet.
    NotAddressable,
    /// The dispatcher or the runtime resync refused; carries the message.
    Failed(String),
}

/// Read the number a text field holds.
///
/// A comma is accepted as the decimal separator: the app ships in nine locales
/// and a pt-BR / fr-FR keyboard types "0,5" for what the parser wants as "0.5".
pub(crate) fn parse_number_text(text: &str) -> Option<f64> {
    text.replace(',', ".").parse::<f64>().ok()
}

/// Commit `value` at `path` on the block the draft points at.
///
/// `Ok(false)` ⇒ the dispatcher accepted the command but reported no change,
/// so there is nothing to resync.
pub(crate) fn apply_block_parameter(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    draft: &Rc<RefCell<Option<BlockEditorDraft>>>,
    path: &str,
    value: ParamValue,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<bool, ApplyParamError> {
    let (chain_index, block_index) = {
        let borrowed = draft.borrow();
        let Some(draft) = borrowed.as_ref() else {
            return Err(ApplyParamError::NotAddressable);
        };
        let Some(block_index) = draft.block_index else {
            return Err(ApplyParamError::NotAddressable);
        };
        (draft.chain_index, block_index)
    };
    let (chain_id, block_id) = {
        let borrowed = project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return Err(ApplyParamError::NotAddressable);
        };
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(chain_index) else {
            return Err(ApplyParamError::NotAddressable);
        };
        let Some(block) = chain.blocks.get(block_index) else {
            return Err(ApplyParamError::NotAddressable);
        };
        (chain.id.clone(), block.id.clone())
    };
    let command = match value {
        ParamValue::Number(value) => Command::Block(BlockCommand::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: block_id,
            path: path.to_string(),
            value,
        }),
        ParamValue::Text(value) => Command::Block(BlockCommand::SetBlockParameterText {
            chain: chain_id.clone(),
            block: block_id,
            path: path.to_string(),
            value,
        }),
        ParamValue::Bool(value) => Command::Block(BlockCommand::SetBlockParameterBool {
            chain: chain_id.clone(),
            block: block_id,
            path: path.to_string(),
            value,
        }),
        ParamValue::Option { value, index } => {
            Command::Block(BlockCommand::SelectBlockParameterOption {
                chain: chain_id.clone(),
                block: block_id,
                path: path.to_string(),
                value,
                index,
            })
        }
    };
    let changed = {
        let borrowed = project_session.borrow();
        let Some(session) = borrowed.as_ref() else {
            return Err(ApplyParamError::NotAddressable);
        };
        session
            .dispatcher
            .dispatch(command)
            .map_err(|e| ApplyParamError::Failed(e.to_string()))?
            .into_iter()
            .any(|event| matches!(event, Event::BlockParameterChanged { .. }))
    };
    if !changed {
        return Ok(false);
    }
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        return Err(ApplyParamError::NotAddressable);
    };
    // #614: the command records the value; the live chain only plays it once
    // its runtime is rebuilt.
    request_chain_sync(session, &chain_id).map_err(|e| ApplyParamError::Failed(e.to_string()))?;
    replace_project_chains(
        project_chains,
        &session.project.borrow(),
        input_chain_devices,
        output_chain_devices,
        &[],
    );
    Ok(true)
}

#[cfg(test)]
#[path = "block_param_apply_tests.rs"]
mod tests;
