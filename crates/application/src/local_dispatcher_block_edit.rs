//! Responsibility: handles the block edit commands.
//! Block-edit handler (file-per-feature; #436 dispatcher split). #328: every
//! structural edit runs on a rule-checked draft (`edit_chain_blocks`), so a
//! refused edit leaves the chain untouched.

use anyhow::Result;

use project::block::{find_block_mut, AudioBlockKind};

use crate::block_path::remove_block;
use crate::command::{BlockCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Block-edit commands: overwrite/remove/move/insert-config.
    pub(crate) fn handle_block_edit(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Block(BlockCommand::OverwriteBlock {
                chain,
                block,
                mut replacement,
            }) => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let Some(b) = find_block_mut(blocks, &block.0) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    // Preserve the original block id; replace kind and enabled.
                    replacement.id = block.clone();
                    *b = replacement;
                    Ok(())
                })?;
                Ok(vec![Event::BlockReplaced { chain, block }])
            }
            Command::Block(BlockCommand::RemoveBlock { chain, block }) => {
                self.edit_chain_blocks(&chain, |blocks| {
                    let Some(removed) = remove_block(blocks, &block) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    if matches!(removed.kind, AudioBlockKind::Split(_)) {
                        return Err(anyhow::anyhow!(
                            "block {:?} is a split: remove it with RemoveSplit, which keeps \
                             path A (RemoveBlock would drop both paths)",
                            block
                        ));
                    }
                    Ok(())
                })?;
                Ok(vec![Event::BlockRemoved { chain, block }])
            }
            Command::Block(BlockCommand::MoveBlock {
                chain,
                block,
                new_position,
            }) => {
                self.with_chain(&chain, |c| {
                    let Some(from_idx) = c.blocks.iter().position(|b| b.id == block) else {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    };
                    let moved = c.blocks.remove(from_idx);
                    let insert_at = new_position.min(c.blocks.len());
                    c.blocks.insert(insert_at, moved);
                    Ok(())
                })?;
                Ok(vec![Event::ChainReloaded { chain }])
            }
            // ── Insert block ──────────────────────────────────────────────────
            Command::Block(BlockCommand::SaveInsertBlock { chain, block, io }) => {
                self.with_block(&chain, &block, |b| match &mut b.kind {
                    project::block::AudioBlockKind::Insert(ref mut ib) => {
                        ib.io = io;
                        Ok(())
                    }
                    _ => Err(anyhow::anyhow!("block {:?} is not an InsertBlock", block)),
                })?;
                Ok(vec![Event::InsertBlockSaved { chain, block }])
            }
            other => {
                unreachable!("handle_block_edit received non-edit command: {other:?}")
            }
        }
    }
}
