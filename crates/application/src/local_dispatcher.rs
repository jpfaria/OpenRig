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

use domain::ids::{BlockId, ChainId};
use project::project::Project;
use project::rig::RigProject;

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
    pub(crate) project: Rc<RefCell<Project>>,
    /// #436: the rig (presets/scenes) used to live only in the GUI and
    /// be mutated by hand in a wiring closure. It now lives behind the
    /// dispatcher so MIDI/MCP/GUI all go through `Command::ApplyRigNav`.
    /// `None` for non-rig sessions (legacy projects) — set via
    /// [`Self::attach_rig`] at project load.
    pub(crate) rig: RefCell<Option<Rc<RefCell<RigProject>>>>,
    /// #22: per-chain block-selection *pair* cursor (left block index).
    /// Dispatcher-owned so a footswitch moves it exactly like the mouse.
    /// Absent ⇒ cursor 0.
    pub(crate) selection: RefCell<std::collections::HashMap<ChainId, usize>>,
}

impl LocalDispatcher {
    /// Create a dispatcher that operates on the given shared `Project` handle.
    ///
    /// The caller (e.g. `adapter-gui`'s `ProjectSession`) should `Rc::clone`
    /// its own project handle and pass it here so both sides share the same
    /// allocation.
    pub fn new(project: Rc<RefCell<Project>>) -> Self {
        Self {
            project,
            rig: RefCell::new(None),
            selection: RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// Share the session's `RigProject` handle so rig-nav commands can
    /// mutate the same allocation the GUI renders from. Idempotent.
    pub fn attach_rig(&self, rig: Rc<RefCell<RigProject>>) {
        *self.rig.borrow_mut() = Some(rig);
    }
}

impl CommandDispatcher for LocalDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        // Pure grouping switch: no logic, just routes each command to the
        // handler that owns its category. Behaviour is byte-identical to the
        // original flat match — each handler runs the original arm body
        // unchanged.
        match cmd {
            Command::SetBlockParameterNumber { .. }
            | Command::SetBlockParameterBool { .. }
            | Command::SetBlockParameterText { .. }
            | Command::SelectBlockParameterOption { .. }
            | Command::PickBlockParameterFile { .. } => self.handle_block_param(cmd),

            Command::ToggleBlockEnabled { .. }
            | Command::ReplaceBlockModel { .. }
            | Command::AddBlock { .. }
            | Command::InsertPrebuiltBlock { .. } => self.handle_block_lifecycle(cmd),

            Command::OverwriteBlock { .. }
            | Command::RemoveBlock { .. }
            | Command::MoveBlock { .. }
            | Command::SaveInsertBlock { .. } => self.handle_block_edit(cmd),

            Command::AddChain { .. }
            | Command::ConfigureChain { .. }
            | Command::RemoveChain { .. }
            | Command::SetChainVolume { .. } => self.handle_chain_crud(cmd),

            Command::MoveChainUp { .. }
            | Command::MoveChainDown { .. }
            | Command::ToggleChainEnabled { .. } => self.handle_chain_order(cmd),

            Command::SaveChain { .. }
            | Command::SaveChainInputEndpoints { .. }
            | Command::SaveChainOutputEndpoints { .. } => self.handle_chain_save(cmd),

            Command::SaveChainIo { .. } | Command::LoadChainPreset { .. } => {
                self.handle_chain_io_replace(cmd)
            }

            Command::SaveProject
            | Command::LoadProject { .. }
            | Command::CreateProject { .. }
            | Command::UpdateProjectName { .. }
            | Command::SaveAudioSettings { .. } => self.handle_project(cmd),

            Command::ApplyRigNav { .. } => self.handle_rig_nav(cmd),

            Command::SelectChainBlock { .. } | Command::ToggleSelectedBlock { .. } => {
                self.handle_block_selection(cmd)
            }

            Command::CaptureRigEdits => self.handle_capture_rig_edits(),
        }
    }

    fn subscribe(&self) -> EventStream {
        // Phase 2 will return a real event stream. For now this is a no-op.
    }
}

impl LocalDispatcher {
    /// Borrow the project mutably, locate `chain` then `block`, and run `f`
    /// against the located block. Centralises the chain-not-found /
    /// block-not-found lookup that every block-scoped arm performed inline.
    ///
    /// Behaviour is byte-identical to the previous inline form: same
    /// `borrow_mut` scope, same `find` order, same error strings, same `?`
    /// propagation point.
    fn with_block<R>(
        &self,
        chain: &ChainId,
        block: &BlockId,
        f: impl FnOnce(&mut project::block::AudioBlock) -> Result<R>,
    ) -> Result<R> {
        let mut proj = self.project.borrow_mut();
        let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == *chain) else {
            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
        };
        let Some(target_block) = target_chain.blocks.iter_mut().find(|b| b.id == *block) else {
            return Err(anyhow::anyhow!("block not found: {:?}", block));
        };
        f(target_block)
    }

    /// Borrow the project mutably, locate `chain`, and run `f` against it.
    /// Centralises the chain-not-found lookup shared by chain-scoped arms.
    ///
    /// Behaviour is byte-identical to the previous inline form: same
    /// `borrow_mut` scope, same `find` order, same error string.
    fn with_chain<R>(
        &self,
        chain: &ChainId,
        f: impl FnOnce(&mut project::chain::Chain) -> Result<R>,
    ) -> Result<R> {
        let mut proj = self.project.borrow_mut();
        let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == *chain) else {
            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
        };
        f(target_chain)
    }

    /// Block-parameter commands: set/select a single parameter on a block.
    fn handle_block_param(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetBlockParameterNumber {
                chain,
                block,
                path,
                value,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_number(b, &path, value)
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SetBlockParameterBool {
                chain,
                block,
                path,
                value,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_bool(b, &path, value)
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SetBlockParameterText {
                chain,
                block,
                path,
                value,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_text(b, &path, &value)
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::SelectBlockParameterOption {
                chain,
                block,
                path,
                value,
                index: _,
            } => {
                self.with_block(&chain, &block, |b| {
                    project::block::param_writer::set_parameter_option(b, &path, &value)
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            Command::PickBlockParameterFile {
                chain,
                block,
                path,
                file,
            } => {
                self.with_block(&chain, &block, |b| {
                    let file_str = file.to_string_lossy();
                    project::block::param_writer::set_parameter_text(b, &path, file_str.as_ref())
                })?;
                Ok(vec![Event::BlockParameterChanged { chain, block, path }])
            }
            other => unreachable!("handle_block_param received non-param command: {other:?}"),
        }
    }

    /// Block-lifecycle commands: enable/model swap/add.
    fn handle_block_lifecycle(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::ToggleBlockEnabled { chain, block } => {
                let new_state = self.with_block(&chain, &block, |b| {
                    b.enabled = !b.enabled;
                    Ok(b.enabled)
                })?;
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
                self.with_block(&chain, &block, |b| {
                    // Preserve id and enabled; replace only the kind (defaults reset).
                    b.kind = new_block.kind;
                    Ok(())
                })?;
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
            Command::InsertPrebuiltBlock {
                chain,
                block,
                position,
            } => {
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

    /// Block-edit commands: overwrite/remove/move/insert-config.
    fn handle_block_edit(&self, cmd: Command) -> Result<Vec<Event>> {
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

    /// Chain CRUD commands: add/configure/remove/volume.
    fn handle_chain_crud(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
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
                self.with_chain(&chain_id, |existing| {
                    // Preserve runtime-only state (enabled) — callers must use
                    // ToggleChainEnabled to change the running state.
                    let keep_enabled = existing.enabled;
                    *existing = chain;
                    existing.enabled = keep_enabled;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainConfigured { chain: chain_id },
                    Event::ProjectMutated,
                ])
            }
            Command::RemoveChain { chain } => {
                {
                    let mut proj = self.project.borrow_mut();
                    let pre_len = proj.chains.len();
                    proj.chains.retain(|c| c.id != chain);
                    if proj.chains.len() == pre_len {
                        return Err(anyhow::anyhow!("chain not found: {:?}", chain));
                    }
                }
                // #436: a rig chain (`rig:<input>`) must also drop its
                // RigInput, else any re-projection resurrects it. This
                // used to be done by hand in the GUI — now it's here.
                if let (Some(rig), Some(name)) =
                    (self.rig.borrow().clone(), chain.0.strip_prefix("rig:"))
                {
                    rig.borrow_mut().remove_input(name);
                }
                Ok(vec![Event::ChainRemoved { chain }, Event::ProjectMutated])
            }
            // ── Chain volume (issue #440) ─────────────────────────────────────
            Command::SetChainVolume { chain, value } => {
                self.with_chain(&chain, |c| {
                    c.volume = value;
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainVolumeChanged { chain, value },
                    Event::ProjectMutated,
                ])
            }
            other => unreachable!("handle_chain_crud received non-crud command: {other:?}"),
        }
    }

    /// Chain ordering / enable commands: move up/down, toggle enabled.
    fn handle_chain_order(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
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
            other => unreachable!("handle_chain_order received non-order command: {other:?}"),
        }
    }

    /// Chain save/upsert + input/output endpoint replacement commands.
    fn handle_chain_save(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
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
                self.with_chain(&chain, |c| {
                    // Remove all existing Input blocks, retaining non-input blocks.
                    c.blocks
                        .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Input(_)));
                    // Insert the new input blocks at the head (inputs-first convention).
                    for (i, blk) in input_blocks.into_iter().enumerate() {
                        c.blocks.insert(i, blk);
                    }
                    Ok(())
                })?;
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
                self.with_chain(&chain, |c| {
                    // Remove all existing Output blocks, retaining non-output blocks.
                    c.blocks
                        .retain(|b| !matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                    // Append the new output blocks at the tail (outputs-last convention).
                    c.blocks.extend(output_blocks);
                    Ok(())
                })?;
                Ok(vec![
                    Event::ChainOutputEndpointsSaved {
                        chain: chain.clone(),
                    },
                    Event::ProjectMutated,
                ])
            }
            other => unreachable!("handle_chain_save received non-save command: {other:?}"),
        }
    }

    /// Chain I/O block replacement + preset load commands.
    fn handle_chain_io_replace(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SaveChainIo {
                chain,
                input_block,
                output_block,
            } => {
                self.with_chain(&chain, |c| {
                    let in_pos = c
                        .blocks
                        .iter()
                        .position(|b| matches!(&b.kind, project::block::AudioBlockKind::Input(_)));
                    let Some(in_idx) = in_pos else {
                        return Err(anyhow::anyhow!(
                            "chain {:?} has no InputBlock to replace",
                            chain
                        ));
                    };
                    let out_pos = c
                        .blocks
                        .iter()
                        .position(|b| matches!(&b.kind, project::block::AudioBlockKind::Output(_)));
                    let Some(out_idx) = out_pos else {
                        return Err(anyhow::anyhow!(
                            "chain {:?} has no OutputBlock to replace",
                            chain
                        ));
                    };
                    c.blocks[in_idx] = input_block;
                    c.blocks[out_idx] = output_block;
                    Ok(())
                })?;
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
                self.with_chain(&chain, |c| {
                    c.blocks = preset_blocks;
                    Ok(())
                })?;
                Ok(vec![Event::ChainPresetLoaded { chain }])
            }
            other => {
                unreachable!("handle_chain_io_replace received non-replace command: {other:?}")
            }
        }
    }

    /// Project lifecycle + settings commands.
    fn handle_project(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
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
            other => unreachable!("handle_project received non-project command: {other:?}"),
        }
    }
}
