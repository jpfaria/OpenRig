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
use crate::chain_validation;
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
            Command::InsertPrebuiltBlock {
                chain,
                block,
                position,
            } => {
                let block_id = block.id.clone();
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let insert_at = position.min(target_chain.blocks.len());
                target_chain.blocks.insert(insert_at, block);
                Ok(vec![Event::BlockAdded {
                    chain,
                    block: block_id,
                }])
            }
            Command::OverwriteBlock {
                chain,
                block,
                mut replacement,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                // Preserve the original block id; replace kind and enabled.
                replacement.id = block.clone();
                *target_block = replacement;
                Ok(vec![Event::BlockReplaced { chain, block }])
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
            // ── Chain CRUD ────────────────────────────────────────────────────
            Command::AddChain { chain } => {
                // Validate that enabling this chain (enabled=true) would not
                // conflict with existing enabled chains.
                // Note: new chains start with enabled=false, so no conflict
                // is possible. We still run the check in case the caller
                // sets enabled=true on the supplied chain.
                if chain.enabled {
                    let proj = self.project.borrow();
                    chain_validation::validate_no_channel_conflict(&proj, &chain, None)
                        .map_err(|e| anyhow::anyhow!("{}", e))?;
                }
                let chain_id = chain.id.clone();
                self.project.borrow_mut().chains.push(chain);
                Ok(vec![
                    Event::ChainAdded { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::ConfigureChain { chain } => {
                let chain_id = chain.id.clone();
                let mut proj = self.project.borrow_mut();
                let Some(existing) = proj.chains.iter_mut().find(|c| c.id == chain_id) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain_id));
                };
                // Preserve runtime-only state (enabled) — callers must use
                // ToggleChainEnabled to change the running state.
                let keep_enabled = existing.enabled;
                *existing = chain;
                existing.enabled = keep_enabled;
                Ok(vec![
                    Event::ChainConfigured { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::RemoveChain { chain } => {
                let mut proj = self.project.borrow_mut();
                let pre_len = proj.chains.len();
                proj.chains.retain(|c| c.id != chain);
                if proj.chains.len() == pre_len {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                }
                Ok(vec![Event::ChainRemoved { chain }, Event::ProjectMutated])
            }
            Command::MoveChainUp { chain } => {
                let mut proj = self.project.borrow_mut();
                let Some(idx) = proj.chains.iter().position(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                if idx == 0 {
                    // Already at the top — no-op, return Ok with no events.
                    return Ok(vec![]);
                }
                proj.chains.swap(idx - 1, idx);
                let new_position = idx - 1;
                Ok(vec![
                    Event::ChainMoved {
                        chain,
                        new_position,
                    },
                    Event::ProjectMutated,
                ])
            }
            Command::MoveChainDown { chain } => {
                let mut proj = self.project.borrow_mut();
                let Some(idx) = proj.chains.iter().position(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                if idx + 1 >= proj.chains.len() {
                    // Already at the bottom — no-op.
                    return Ok(vec![]);
                }
                proj.chains.swap(idx, idx + 1);
                let new_position = idx + 1;
                Ok(vec![
                    Event::ChainMoved {
                        chain,
                        new_position,
                    },
                    Event::ProjectMutated,
                ])
            }
            Command::ToggleChainEnabled { chain } => {
                // Phase 1: determine current state (immutable borrow).
                let (will_enable, chain_clone) = {
                    let proj = self.project.borrow();
                    let Some(target) = proj.chains.iter().find(|c| c.id == chain) else {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    };
                    (!target.enabled, target.clone())
                };
                // Phase 2: if enabling, validate no channel conflict
                // (skip self so the chain doesn't conflict with its own current state).
                if will_enable {
                    let proj = self.project.borrow();
                    chain_validation::validate_no_channel_conflict(
                        &proj,
                        &chain_clone,
                        Some(&chain),
                    )
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                }
                // Phase 3: mutate.
                {
                    let mut proj = self.project.borrow_mut();
                    let target = proj.chains.iter_mut().find(|c| c.id == chain).unwrap();
                    target.enabled = will_enable;
                }
                Ok(vec![Event::ChainEnabledChanged {
                    chain,
                    enabled: will_enable,
                }])
            }
            // ── Insert block ──────────────────────────────────────────────────
            Command::SaveInsertBlock {
                chain,
                block,
                send,
                return_,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == block)
                else {
                    return Err(anyhow::anyhow!("block not found: {:?}", block));
                };
                match &mut target_block.kind {
                    project::block::AudioBlockKind::Insert(ref mut ib) => {
                        ib.send = send;
                        ib.return_ = return_;
                    }
                    _ => {
                        return Err(anyhow::anyhow!("block {:?} is not an InsertBlock", block));
                    }
                }
                Ok(vec![Event::InsertBlockSaved { chain, block }])
            }

            // ── Chain save (upsert) ───────────────────────────────────────────
            Command::SaveChain { chain } => {
                let chain_id = chain.id.clone();
                let mut proj = self.project.borrow_mut();
                if let Some(existing) = proj.chains.iter_mut().find(|c| c.id == chain_id) {
                    // Replace in-place, preserving the running enabled state.
                    let keep_enabled = existing.enabled;
                    *existing = chain;
                    existing.enabled = keep_enabled;
                } else {
                    // Append (create flow — chain_id not found in project yet).
                    proj.chains.push(chain);
                }
                Ok(vec![
                    Event::ChainSaved { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }

            // ── Chain I/O endpoints ───────────────────────────────────────────
            Command::SaveChainInputEndpoints {
                chain,
                input_blocks,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                // Remove all existing Input blocks, retaining non-input blocks.
                target_chain
                    .blocks
                    .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Input(_)));
                // Insert the new input blocks at the head (inputs-first convention).
                for (i, blk) in input_blocks.into_iter().enumerate() {
                    target_chain.blocks.insert(i, blk);
                }
                Ok(vec![
                    Event::ChainInputEndpointsSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            Command::SaveChainOutputEndpoints {
                chain,
                output_blocks,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                // Remove all existing Output blocks, retaining non-output blocks.
                target_chain
                    .blocks
                    .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                // Append the new output blocks at the tail (outputs-last convention).
                target_chain.blocks.extend(output_blocks);
                Ok(vec![
                    Event::ChainOutputEndpointsSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            Command::SaveChainIo {
                chain,
                input_block,
                output_block,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                let in_pos = target_chain
                    .blocks
                    .iter()
                    .position(|b| matches!(&b.kind, project::block::AudioBlockKind::Input(_)));
                let Some(in_idx) = in_pos else {
                    return Err(anyhow::anyhow!(
                        "chain {:?} has no InputBlock to replace",
                        chain
                    ));
                };
                let out_pos = target_chain
                    .blocks
                    .iter()
                    .position(|b| matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                let Some(out_idx) = out_pos else {
                    return Err(anyhow::anyhow!(
                        "chain {:?} has no OutputBlock to replace",
                        chain
                    ));
                };
                target_chain.blocks[in_idx] = input_block;
                target_chain.blocks[out_idx] = output_block;
                Ok(vec![
                    Event::ChainIoSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }

            // ── Chain presets ─────────────────────────────────────────────────
            Command::LoadChainPreset {
                chain,
                preset_blocks,
            } => {
                let mut proj = self.project.borrow_mut();
                let Some(target) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                };
                target.blocks = preset_blocks;
                Ok(vec![Event::ChainPresetLoaded { chain }])
            }

            // ── Project lifecycle ─────────────────────────────────────────────
            // File I/O happens in the adapter before dispatch. The dispatcher
            // signals the completion via events only.
            Command::SaveProject => Ok(vec![Event::ProjectSaved]),

            Command::LoadProject { project, path: _ } => {
                // Replace the shared project data in-place so all Rc::clone
                // holders (adapter-gui's ProjectSession) see the updated state.
                *self.project.borrow_mut() = project;
                Ok(vec![Event::ProjectLoaded, Event::ProjectMutated])
            }

            Command::CreateProject { project } => {
                *self.project.borrow_mut() = project;
                Ok(vec![Event::ProjectCreated, Event::ProjectMutated])
            }

            // ── Project settings ──────────────────────────────────────────────
            Command::UpdateProjectName { name } => {
                let trimmed = name.trim().to_string();
                self.project.borrow_mut().name = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                };
                Ok(vec![Event::ProjectMutated])
            }

            Command::SaveAudioSettings { device_settings } => {
                self.project.borrow_mut().device_settings = device_settings;
                Ok(vec![Event::AudioSettingsSaved])
            }
        }
    }

    fn subscribe(&self) -> EventStream {
        // Phase 2 will return a real event stream. For now this is a no-op.
    }
}
