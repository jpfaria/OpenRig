//! `LocalDispatcher` — in-process implementation of `CommandDispatcher`.
//!
//! Holds the project session via interior mutability so `dispatch` can take
//! `&self` (required by the trait; callers may hold multiple references).
//!
//! **Current state (Phase 1 skeleton):** every `Command` arm is
//! `unimplemented!("phase-1 task pending")`. This is intentional — no
//! production caller dispatches these arms yet because adapter-gui has not
//! been migrated. Tasks 2..N will fill the arms one by one, each accompanied
//! by its own failing test that drives the implementation (TDD).
//!
//! `unimplemented!()` is acceptable here because the arms are unreachable
//! from production code in this state. The forbidden pattern is
//! `unimplemented!()` on arms that live callers can reach.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;

use crate::command::Command;
use crate::dispatcher::{CommandDispatcher, EventStream};
use crate::event::Event;
use crate::session::ApplicationSession;

/// In-process dispatcher backed by an `ApplicationSession`.
///
/// Uses `Rc<RefCell<_>>` for interior mutability on the main (UI) thread.
/// This is NOT `Send` — see the note in `dispatcher.rs` about deferred
/// `Send + Sync` bounds.
pub struct LocalDispatcher {
    pub project_session: Rc<RefCell<ApplicationSession>>,
}

impl LocalDispatcher {
    pub fn new(session: ApplicationSession) -> Self {
        Self {
            project_session: Rc::new(RefCell::new(session)),
        }
    }
}

impl CommandDispatcher for LocalDispatcher {
    fn dispatch(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetBlockParameterNumber { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SetBlockParameterBool { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SetBlockParameterText { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::SelectBlockParameterOption { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::PickBlockParameterFile { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::ToggleBlockEnabled { chain, block } => {
                let mut session = self.project_session.borrow_mut();
                let Some(target_chain) = session.project.chains.iter_mut().find(|c| c.id == chain)
                else {
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
            Command::ReplaceBlockModel { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::AddBlock { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::RemoveBlock { .. } => {
                unimplemented!("phase-1 task pending")
            }
            Command::MoveBlock { .. } => {
                unimplemented!("phase-1 task pending")
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
