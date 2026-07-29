//! Block-lifecycle handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use domain::ids::BlockId;

use crate::block_factory::{build_default_block, resolve_effect_type_for_model};
use crate::command::{BlockCommand, Command};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Block-lifecycle commands: enable/model swap/add.
    pub(crate) fn handle_block_lifecycle(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Block(BlockCommand::ToggleBlockEnabled { chain, block }) => {
                let new_state = self.with_block(&chain, &block, |b| {
                    // #606: never enable a block whose model is unavailable —
                    // the user cannot activate a pedal whose pack is not
                    // installed (or is unsupported on this platform). Disabling
                    // an already-on block is always allowed.
                    if !b.enabled
                        && !project::project_disable_unavailable::block_model_is_available(&b.kind)
                    {
                        return Ok(false);
                    }
                    b.enabled = !b.enabled;
                    Ok(b.enabled)
                })?;
                // #548: mirror into SelectionState when this is the
                // active block so MIDI slot `toggle_active_block_enabled`
                // sees the truth on the next press.
                if let Ok(mut s) = self.selection_state.write() {
                    let is_active_chain = s.active_chain.as_deref() == Some(chain.0.as_str());
                    let is_active_block = s.active_block.as_deref() == Some(block.0.as_str());
                    if is_active_chain && is_active_block {
                        s.active_block_enabled = new_state;
                    }
                }
                Ok(vec![Event::BlockEnabledChanged {
                    chain,
                    block,
                    enabled: new_state,
                }])
            }
            Command::Block(BlockCommand::ReplaceBlockModel {
                chain,
                block,
                model_id,
            }) => {
                // Resolve the effect_type for the given model_id by scanning the registry.
                let effect_type = resolve_effect_type_for_model(&model_id)?;
                // Build a fresh block with default params for the new model.
                let new_block = build_default_block(
                    BlockId(String::new()), // placeholder; we preserve the existing id below
                    &effect_type,
                    &model_id,
                )?;
                self.with_block(&chain, &block, |b| {
                    // Preserve id and enabled; replace only the kind (defaults reset).
                    b.kind = new_block.kind;
                    Ok(())
                })?;
                Ok(vec![Event::BlockReplaced { chain, block }])
            }
            Command::Block(BlockCommand::AddBlock {
                chain,
                kind,
                model_id,
                position,
            }) => {
                // Build the new block with default params before mutating the project.
                // A unique id is generated from the chain id + current timestamp-ish counter.
                let block_id = {
                    let proj = self.project.borrow();
                    let chain_ref = proj.chains.iter().find(|c| c.id == chain);
                    let n = chain_ref.map(|c| c.blocks.len()).unwrap_or(0);
                    BlockId(format!("{}:{}:{}", chain.0, kind, n))
                };
                let new_block = build_default_block(block_id, &kind, &model_id)?;
                let new_block_id = new_block.id.clone();
                self.with_chain(&chain, |c| {
                    let insert_at = position.min(c.blocks.len());
                    c.blocks.insert(insert_at, new_block);
                    Ok(())
                })?;
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: new_block_id,
                }])
            }
            Command::Block(BlockCommand::InsertPrebuiltBlock {
                chain,
                block,
                position,
            }) => {
                let block_id = block.id.clone();
                self.with_chain(&chain, |c| {
                    let insert_at = position.min(c.blocks.len());
                    c.blocks.insert(insert_at, block);
                    Ok(())
                })?;
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: block_id,
                }])
            }
            other => {
                unreachable!("handle_block_lifecycle received non-lifecycle command: {other:?}")
            }
        }
    }
}
