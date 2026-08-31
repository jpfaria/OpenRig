//! Responsibility: saves a chain as a named preset.
//!
//! Split out of `preset_save_wiring` (#913). Which overlay is on screen is the
//! window's business; what gets saved, under what name, and what the user's
//! typing means is not:
//!
//! * an empty field means "keep the default", not "save as empty";
//! * saving also renames the active preset, or the chain-title combobox keeps
//!   the old label and the user reads it as "nothing happened".

use std::rc::Rc;

use application::command::{ChainCommand, Command, SelectionCommand};
use domain::ids::ChainId;

use crate::chain_preset_wiring::default_preset_filename_slug;
use crate::state::ProjectSession;

/// State carried across the in-window save flow: open the overlay, maybe
/// confirm an overwrite, then commit.
#[derive(Clone, Debug)]
pub(crate) struct PendingSave {
    pub(crate) chain_id: ChainId,
    pub(crate) default_name: String,
}

/// Why the flow could not even start.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PresetSaveError {
    /// The row index is not a chain of this project.
    NoSuchChain,
}

/// Resolve what a save of the chain at `index` would write.
///
/// #518: the default name is the ACTIVE PRESET's name, not the chain's title —
/// the title moved to `input.label` in #436, so reusing it named the file after
/// the chain instead of the tone. A chain that is not projected from a rig
/// input falls back to its own description.
pub(crate) fn pending_save_for(
    session: &ProjectSession,
    index: usize,
) -> Result<PendingSave, PresetSaveError> {
    let (description, chain_id) = {
        let project = session.project.borrow();
        let Some(chain) = project.chains.get(index) else {
            return Err(PresetSaveError::NoSuchChain);
        };
        (
            chain
                .description
                .clone()
                .unwrap_or_else(|| format!("chain_{}", index + 1)),
            chain.id.clone(),
        )
    };
    let default_name = session
        .rig
        .as_ref()
        .and_then(|rig| default_preset_filename_slug(&chain_id, &rig.borrow()))
        .unwrap_or(description);
    Ok(PendingSave {
        chain_id,
        default_name,
    })
}

/// What name the save actually uses. An empty or blank field keeps the default
/// — the user clearing it means "I did not want to rename it", not "save this
/// preset as nothing".
pub(crate) fn chosen_name(typed: &str, default_name: &str) -> String {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        default_name.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Commit the save: write the preset through the bus, then rename the active
/// preset to match.
///
/// #555: the YAML write and `create_dir_all` live inside the dispatcher's
/// handler, so MCP/MIDI/gRPC produce the same on-disk effect as the GUI.
pub(crate) fn commit_preset_save(
    session: &ProjectSession,
    chain_id: &ChainId,
    name: &str,
) -> Result<(), String> {
    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::SaveChainPreset {
            chain: chain_id.clone(),
            name: name.to_string(),
        }))
        .map_err(|e| e.to_string())?;
    if let Err(e) =
        session
            .dispatcher
            .dispatch(Command::Selection(SelectionCommand::RenameRigPreset {
                chain: chain_id.clone(),
                name: name.to_string(),
            }))
    {
        // The preset IS saved; only the label lags. Worth a line, not a failure.
        log::warn!("[preset-save] Command::RenameRigPreset failed: {e}");
    }
    Ok(())
}

/// Keeps the `Rc` import honest for callers that hold the pending state.
pub(crate) type PendingSaveCell = Rc<std::cell::RefCell<Option<PendingSave>>>;

#[cfg(test)]
#[path = "preset_save_tests.rs"]
mod tests;
