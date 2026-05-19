//! Block-edit handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Block-edit commands: overwrite/remove/move/insert-config.
    pub(crate) fn handle_block_edit(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::OverwriteBlock {
                chain,
                block,
                mut replacement,
            } => {
                self.with_block(&chain, &block, |b| {
                    // Preserve the original block id; replace kind and enabled.
                    replacement.id = block.clone();
                    *b = replacement;
                    Ok(())
                })?;
                Ok(vec![Event::BlockReplaced { chain, block }])
            }
            Command::RemoveBlock { chain, block } => {
                self.with_chain(&chain, |c| {
                    let pre_len = c.blocks.len();
                    c.blocks.retain(|b| b.id != block);
                    if c.blocks.len() == pre_len {
                        return Err(anyhow::anyhow!("block not found: {:?}", block));
                    }
                    Ok(())
                })?;
                Ok(vec![Event::BlockRemoved { chain, block }])
            }
            Command::MoveBlock {
                chain,
                block,
                new_position,
            } => {
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
            Command::SaveInsertBlock {
                chain,
                block,
                send,
                return_,
            } => {
                self.with_block(&chain, &block, |b| match &mut b.kind {
                    project::block::AudioBlockKind::Insert(ref mut ib) => {
                        ib.send = send;
                        ib.return_ = return_;
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
