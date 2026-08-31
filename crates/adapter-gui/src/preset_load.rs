//! Responsibility: loads a preset file onto a chain.
//!
//! Split out of `chain_preset_wiring` (#913). Closing the picker is screen
//! work; what lands on the chain is not:
//!
//! * the blocks are handed over I/O-STRIPPED — the dispatcher preserves the
//!   chain's existing Input/Output across the swap, so wrapping I/O here too
//!   put two of each on the chain (#518);
//! * every block gets a FRESH id, or two chains loaded from the same file
//!   would share block ids;
//! * the active preset is renamed to the file's stem, or the combobox keeps
//!   the old label and the user reads it as "nothing happened" (#510).

use std::cell::RefCell;
use std::rc::Rc;

use application::command::{ChainCommand, Command, SelectionCommand};
use domain::ids::ChainId;
use domain::AudioDeviceDescriptor;
use project::chain::Chain;
use slint::VecModel;
use std::path::Path;

use crate::chain_block_helpers::assign_new_block_ids;
use crate::chain_preset_bank::{preset_rename_target_from_path, strip_io_blocks};
use crate::project_session_load::load_preset_file;
use crate::project_view::replace_project_chains;
use crate::runtime_sync_policy::request_chain_sync;
use crate::state::ProjectSession;
use crate::ProjectChainItem;

/// Why the load did not happen.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PresetLoadError {
    /// No project open, or the picker's chain index no longer resolves.
    Gone,
    /// The file could not be read or parsed; carries the message.
    Unreadable(String),
    /// The dispatcher or the runtime resync refused; carries the message.
    Failed(String),
}

/// Load `path` onto the chain at `chain_index` and republish the chain rows.
pub(crate) fn load_preset_onto_chain(
    project_session: &Rc<RefCell<Option<ProjectSession>>>,
    chain_index: usize,
    path: &Path,
    project_chains: &Rc<VecModel<ProjectChainItem>>,
    input_chain_devices: &[AudioDeviceDescriptor],
    output_chain_devices: &[AudioDeviceDescriptor],
) -> Result<ChainId, PresetLoadError> {
    let preset = load_preset_file(path).map_err(|e| PresetLoadError::Unreadable(e.to_string()))?;
    let mut borrowed = project_session.borrow_mut();
    let Some(session) = borrowed.as_mut() else {
        return Err(PresetLoadError::Gone);
    };
    let Some(chain_id) = session
        .project
        .borrow()
        .chains
        .get(chain_index)
        .map(|chain| chain.id.clone())
    else {
        return Err(PresetLoadError::Gone);
    };

    // Fresh ids via a throwaway chain, so the same file loaded onto two chains
    // does not give them the same block ids.
    let mut staged = Chain {
        id: chain_id.clone(),
        description: None,
        instrument: String::new(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: strip_io_blocks(preset.blocks),
        di_output: None,
        loopers: vec![],
    };
    assign_new_block_ids(&mut staged);

    session
        .dispatcher
        .dispatch(Command::Chain(ChainCommand::LoadChainPreset {
            chain: chain_id.clone(),
            preset_instrument: preset.instrument.clone(),
            preset_blocks: staged.blocks,
        }))
        .map_err(|e| PresetLoadError::Failed(e.to_string()))?;

    // #510 round-trip contract: the active preset's name follows the file the
    // user picked, verbatim.
    if let Some(name) = preset_rename_target_from_path(path) {
        if let Err(e) =
            session
                .dispatcher
                .dispatch(Command::Selection(SelectionCommand::RenameRigPreset {
                    chain: chain_id.clone(),
                    name,
                }))
        {
            log::warn!("[preset] Command::RenameRigPreset failed: {e}");
        }
    }
    request_chain_sync(session, &chain_id).map_err(|e| PresetLoadError::Failed(e.to_string()))?;
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
#[path = "preset_load_tests.rs"]
mod tests;
