//! Responsibility: handles the split lifecycle commands.
//!
//! #328 (spec §3): `AddSplit`, `SetSplitEnd` and `RemoveSplit` reshape the
//! chain's top-level block list around the split they name. Each runs through
//! `edit_chain_blocks`, so the split rules (at most one Mix and one Y, the Y
//! last) are checked on the result and a refused command changes nothing.

use anyhow::{anyhow, Result};

use domain::ids::BlockId;
use project::block::split_params::default_split_params;
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};

use crate::command::{Command, SplitCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

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
            } => {
                let split_id = BlockId::generate_for_chain(&chain);
                let block = empty_split(split_id.clone(), end);
                self.edit_chain_blocks(&chain, |blocks| {
                    let at = position.min(blocks.len());
                    blocks.insert(at, block);
                    Ok(())
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
            SplitCommand::RemoveSplit { chain, split_id } => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let at = blocks
                        .iter()
                        .position(|b| b.id == split_id)
                        .ok_or_else(|| anyhow!("split not found: {:?}", split_id))?;
                    let AudioBlockKind::Split(removed) = blocks.remove(at).kind else {
                        return Err(anyhow!("block {:?} is not a split", split_id));
                    };
                    // Path A takes the split's place; path B goes with it.
                    let tail = blocks.split_off(at);
                    blocks.extend(removed.a);
                    blocks.extend(tail);
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
        kind: AudioBlockKind::Split(SplitBlock {
            end,
            params: default_split_params(),
            a: Vec::new(),
            b: Vec::new(),
        }),
    }
}

fn split_mut<'a>(blocks: &'a mut [AudioBlock], split_id: &BlockId) -> Result<&'a mut SplitBlock> {
    let block = blocks
        .iter_mut()
        .find(|b| b.id == *split_id)
        .ok_or_else(|| anyhow!("split not found: {:?}", split_id))?;
    match &mut block.kind {
        AudioBlockKind::Split(split) => Ok(split),
        _ => Err(anyhow!("block {:?} is not a split", split_id)),
    }
}
