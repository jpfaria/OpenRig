//! Chain ordering/enable handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

use crate::chain_validation;
use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Chain ordering / enable commands: move up/down, toggle enabled.
    pub(crate) fn handle_chain_order(&self, cmd: Command) -> Result<Vec<Event>> {
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
                // #548: if this is the active chain, mirror the new
                // enabled state into the SelectionState snapshot so MIDI
                // slot `toggle_active_chain_enabled` reads the truth on
                // the next press.
                if let Ok(mut s) = self.selection_state.write() {
                    if s.active_chain.as_deref() == Some(chain.0.as_str()) {
                        s.active_chain_enabled = will_enable;
                    }
                }
                Ok(vec![Event::ChainEnabledChanged {
                    chain,
                    enabled: will_enable,
                }])
            }
            other => unreachable!("handle_chain_order received non-order command: {other:?}"),
        }
    }
}
