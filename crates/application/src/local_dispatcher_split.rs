//! Responsibility: handles the split lifecycle commands.
//!
//! #328 (spec §3, §10, §11): `AddSplit`, `SetSplitEnd`, `RemoveSplit` and the
//! path commands reshape the list that holds the split they name, at any depth. Each runs through
//! `edit_chain_blocks`, so the split rules (a Y ends its own list) are checked
//! on the result and a refused command changes nothing.

use anyhow::{anyhow, Result};

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};

use crate::block_path::{insert_block, list_holding, split_mut};
use crate::command::{Command, SplitCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::split_path_commands::add_split_path;

impl LocalDispatcher {
    pub(crate) fn handle_split(&self, cmd: Command) -> Result<Vec<Event>> {
        let Command::Split(split_cmd) = cmd else {
            unreachable!("handle_split received a non-split command: {cmd:?}");
        };
        match split_cmd {
            SplitCommand::AddSplit {
                chain,
                position,
                end,
                path,
            } => {
                let split_id = BlockId::generate_for_chain(&chain);
                let block = empty_split(split_id.clone(), end);
                self.edit_chain_blocks(&chain, |blocks| {
                    insert_block(blocks, path.as_ref(), position, block)
                })?;
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: split_id,
                }])
            }
            SplitCommand::SetSplitEnd {
                chain,
                split_id,
                end,
            } => {
                self.edit_chain_blocks(&chain, |blocks| {
                    split_mut(blocks, &split_id)?.end = end;
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
            SplitCommand::AddSplitPath { chain, split_id } => {
                self.edit_chain_blocks(&chain, |blocks| {
                    add_split_path(split_mut(blocks, &split_id)?);
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
            SplitCommand::RemoveSplitPath {
                chain,
                split_id,
                path,
            } => {
                self.remove_split_path(&chain, &split_id, path)?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
            SplitCommand::RemoveSplit { chain, split_id } => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let (list, at) = list_holding(blocks, &split_id)
                        .ok_or_else(|| anyhow!("split not found: {:?}", split_id))?;
                    let AudioBlockKind::Split(removed) = list.remove(at).kind else {
                        return Err(anyhow!("block {:?} is not a split", split_id));
                    };
                    // Path 0 takes the split's place; the other paths go with it.
                    let tail = list.split_off(at);
                    list.extend(removed.paths.into_iter().next().unwrap_or_default());
                    list.extend(tail);
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
        }
    }
}

fn empty_split(id: BlockId, end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id,
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::new(end)),
    }
}
