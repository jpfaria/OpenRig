//! Responsibility: handles the split lifecycle commands.
//!
//! #328 (spec §3, §10, §11): `AddSplit`, `SetSplitEnd`, `RemoveSplit` and the
//! path commands reshape the list that holds the split they name, at any depth. Each runs through
//! `edit_chain_blocks`, so the split rules (a Y ends its own list) are checked
//! on the result and a refused command changes nothing.

use anyhow::{anyhow, Result};

use domain::ids::{BlockId, ChainId};
use project::block::{walk_blocks, AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};

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
                if end != SplitEnd::Y {
                    // No longer a Y: its leaves, and their checklists, are gone.
                    self.forget_leaves(&chain, &[split_id])?;
                }
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
                let gone = self.edit_chain_blocks(&chain, |blocks| {
                    let (list, at) = list_holding(blocks, &split_id)
                        .ok_or_else(|| anyhow!("split not found: {:?}", split_id))?;
                    let AudioBlockKind::Split(removed) = list.remove(at).kind else {
                        return Err(anyhow!("block {:?} is not a split", split_id));
                    };
                    // Path 0 takes the split's place; the other paths go with it.
                    let mut paths = removed.paths.into_iter();
                    let tail = list.split_off(at);
                    list.extend(paths.next().unwrap_or_default());
                    list.extend(tail);
                    let dropped: Vec<AudioBlock> = paths.flatten().collect();
                    Ok(std::iter::once(split_id.clone())
                        .chain(walk_blocks(&dropped).into_iter().map(|b| b.id.clone()))
                        .collect::<Vec<_>>())
                })?;
                self.forget_leaves(&chain, &gone)?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
        }
    }

    /// Forget the leaf checklists of the splits in `gone`.
    fn forget_leaves(&self, chain: &ChainId, gone: &[BlockId]) -> Result<()> {
        self.with_chain(chain, |c| {
            c.disabled_endpoints.forget_splits(gone);
            Ok(())
        })
    }
}

fn empty_split(id: BlockId, end: SplitEnd) -> AudioBlock {
    AudioBlock {
        id,
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::new(end)),
    }
}
