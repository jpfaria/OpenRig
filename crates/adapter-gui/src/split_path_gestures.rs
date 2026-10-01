//! Responsibility: decides what a "+ path" or remove-path gesture asks the bus for.
//!
//! #328 (spec §11): the split editor adds a path to the split it shows, or
//! removes one. A path that holds blocks takes them with it, so its removal
//! asks for a confirmation first; an empty path goes straight away. A split
//! never drops below `MIN_SPLIT_PATHS` — the command refuses it, and the
//! editor hides the remove buttons there.

use application::command::{Command, SplitCommand};
use domain::ids::BlockId;
use project::block::{path_letter, MIN_SPLIT_PATHS};
use project::chain::Chain;

use crate::chain_block_lists::split_by_id;

/// What a remove-path gesture turns into.
#[derive(Debug, Clone)]
pub(crate) enum RemovePathRequest {
    /// The path is empty: dispatch at once.
    Direct(Command),
    /// The path holds blocks: confirm first, naming the path in the dialog.
    Confirm { command: Command, name: String },
}

/// The `AddSplitPath` for split `split_id` of `chain`, when the chain has it.
pub(crate) fn add_path_command(chain: &Chain, split_id: &BlockId) -> Option<Command> {
    split_by_id(chain, split_id)?;
    Some(Command::Split(SplitCommand::AddSplitPath {
        chain: chain.id.clone(),
        split_id: split_id.clone(),
    }))
}

/// The request removing path `path` of split `split_id`, or `None` when the
/// split or the path is missing or the split is already at its minimum.
pub(crate) fn remove_path_request(
    chain: &Chain,
    split_id: &BlockId,
    path: usize,
) -> Option<RemovePathRequest> {
    let (_, split) = split_by_id(chain, split_id)?;
    if split.paths.len() <= MIN_SPLIT_PATHS {
        return None;
    }
    let blocks = split.paths.get(path)?.len();
    let command = Command::Split(SplitCommand::RemoveSplitPath {
        chain: chain.id.clone(),
        split_id: split_id.clone(),
        path,
    });
    if blocks == 0 {
        return Some(RemovePathRequest::Direct(command));
    }
    let name = rust_i18n::t!(
        "confirm-remove-path-name",
        path = path_letter(path),
        n = blocks
    )
    .to_string();
    Some(RemovePathRequest::Confirm { command, name })
}

/// The letters of the paths of split `split_id` (A, B, …), empty when the
/// chain does not have it.
pub(crate) fn path_letters(chain: &Chain, split_id: &BlockId) -> Vec<String> {
    split_by_id(chain, split_id)
        .map(|(_, split)| (0..split.paths.len()).map(path_letter).collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "split_path_gestures_tests.rs"]
mod tests;
