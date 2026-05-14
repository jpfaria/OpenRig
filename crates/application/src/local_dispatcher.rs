//! `LocalDispatcher` — in-process implementation of `CommandDispatcher`.
//!
//! Holds the project via `Rc<RefCell<Project>>` for interior mutability so
//! `dispatch` can take `&self` (required by the trait; callers may hold
//! multiple references to the same dispatcher or to the same project).
//!
//! `adapter-gui`'s `ProjectSession` shares its project handle with this
//! dispatcher so both sides always see the same `Project` data with no extra
//! sync step.
//!
//! **Current state (Phase 1 skeleton):** every `Command` arm except
//! `ToggleBlockEnabled` is `unimplemented!("phase-1 task pending")`.  This is
//! intentional — no production caller dispatches those arms yet because
//! adapter-gui migration is ongoing.  Tasks 4..N will fill the arms one by
//! one, each accompanied by its own failing test that drives the
//! implementation (TDD).
//!
//! `unimplemented!()` is acceptable here because the arms are unreachable
//! from production code in this state.  The forbidden pattern is
//! `unimplemented!()` on arms that live callers can reach.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;

use domain::ids::BlockId;
use project::project::Project;

use crate::block_factory::{build_default_block, resolve_effect_type_for_model};
use crate::command::Command;
use crate::dispatcher::{CommandDispatcher, EventStream};
use crate::event::Event;

/// In-process dispatcher backed by a shared `Project`.
///
/// Uses `Rc<RefCell<_>>` for interior mutability on the main (UI) thread.
/// This is NOT `Send` — see the note in `dispatcher.rs` about deferred
/// `Send + Sync` bounds.
pub struct LocalDispatcher {
    project: Rc<RefCell<Project>>,
}

impl LocalDispatcher {
    /// Create a dispatcher that operates on the given shared `Project` handle.
    ///
    /// The caller (e.g. `adapter-gui`'s `ProjectSession`) should `Rc::clone`
    /// its own project handle and pass it here so both sides share the same
    /// allocation.
    pub fn new(project: Rc<RefCell<Project>>) -> Self {
        Self { project }
    }
}

impl CommandDispatcher for LocalDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetBlockParameterNumber {
                chain,
                block,
                path,
                value,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                project::block::param_writer::set_parameter_number(target_block, &path, value)?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SetBlockParameterBool {
                chain,
                block,
                path,
                value,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                project::block::param_writer::set_parameter_bool(target_block, &path, value)?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SetBlockParameterText {
                chain,
                block,
                path,
                value,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                project::block::param_writer::set_parameter_text(target_block, &path, &value)?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SelectBlockParameterOption {
                chain,
                block,
                path,
                value,
                index: _,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                project::block::param_writer::set_parameter_option(target_block, &path, &value)?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::PickBlockParameterFile {
                chain,
                block,
                path,
                file,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                let file_str = file.to_string_lossy();
                project::block::param_writer::set_parameter_text(
                    target_block,
                    &path,
                    file_str.as_ref(),
                )?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::ToggleBlockEnabled { chain, block } => {
                let mut project = self.project.borrow_mut();
                let Some(target_chain) = project.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                target_block.enabled = !target_block.enabled;
                let new_state = target_block.enabled;
                Ok(vec![Event::BlockEnabledChanged {
                    chain,
                    block,
                    enabled: new_state,
                }])
            }
            Command::ReplaceBlockModel {
                chain,
                block,
                model_id,
            } => {
                // Resolve the effect_type for the given model_id by scanning the registry.
                let effect_type = resolve_effect_type_for_model(&model_id)?;
                // Build a fresh block with default params for the new model.
                let new_block = build_default_block(
                    BlockId(String::new()), // placeholder; we preserve the existing id below
                    &effect_type,
                    &model_id,
                )?;
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                // Preserve id and enabled; replace only the kind (defaults reset).
                target_block.kind = new_block.kind;
                Ok(vec![Event::BlockReplaced { chain, block }])
            }
            Command::AddBlock {
                chain,
                kind,
                model_id,
                position,
            } => {
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
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let insert_at = position.min(target_chain.blocks.len());
                target_chain.blocks.insert(insert_at, new_block);
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: new_block_id,
                }])
            }
            Command::RemoveBlock { chain, block } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let pre_len = target_chain.blocks.len();
                target_chain.blocks.retain(|b| b.id != block);
                if target_chain.blocks.len() == pre_len {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                }
                Ok(vec![Event::BlockRemoved { chain, block }])
            }
            Command::MoveBlock {
                chain,
                block,
                new_position,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(from_idx) = target_chain.blocks.iter().position(|b| b.id == block) else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                let moved = target_chain.blocks.remove(from_idx);
                let insert_at = new_position.min(target_chain.blocks.len());
                target_chain.blocks.insert(insert_at, moved);
                Ok(vec![Event::ChainReloaded { chain }])
            }
            Command::SaveBlockEditorDraft { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveInsertBlock { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveChain { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::RemoveChain { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::MoveChainUp { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::MoveChainDown { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::ToggleChainEnabled { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveChainInputEndpoints { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveChainOutputEndpoints { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveChainIo { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::LoadChainPreset { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveProject => {
                unimplemented!("phase-1 task pending")
            }
            Command::LoadProject { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::CreateProject { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::UpdateProjectName { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SaveAudioSettings => {
                unimplemented!("phase-1 task pending")
            }
        }
    }

    fn subscribe(&self) -> EventStream {
        // Phase 2 will return a real event stream. For now this is a no-op.
    }
}
