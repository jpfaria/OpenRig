//! Chain ordering/enable handler (file-per-feature; #436 dispatcher split).
//! Behaviour byte-identical to the original inline arm — pure move.

use anyhow::Result;

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
                // Phase 2: if enabling, enforce the domain rule + no conflict.
                if will_enable {
                    // #716 domain rule: a chain with no I/O (no binding, no input)
                    // routes nothing — refuse to enable it.
                    if !chain_clone.has_io() {
                        return Err(anyhow::anyhow!(
                            "chain '{}' has no I/O binding — relate an I/O binding before enabling it",
                            chain.0
                        ));
                    }
                    // #716 (model A): the per-block cross-chain channel-conflict
                    // check is gone — device endpoints are resolved from the
                    // per-machine binding registry at activation, where the
                    // conflict check now belongs.
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
