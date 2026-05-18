//! #22: the per-chain block-selection pair cursor handler. The cursor
//! lives behind the dispatcher (not the GUI) so a footswitch moves it
//! exactly like the mouse. `SelectChainBlock` steps the cursor
//! (wrapping); `ToggleSelectedBlock` flips one side of the pair. No
//! audio code — selection is pure model + an event for the UI border.

use anyhow::Result;

use crate::command::{Command, PairSide};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_block_selection(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SelectChainBlock { chain, delta } => {
                let n = match self
                    .project
                    .borrow()
                    .chains
                    .iter()
                    .find(|c| c.id == chain)
                {
                    Some(c) => c.blocks.len(),
                    None => return Ok(vec![]),
                };
                if n == 0 {
                    return Ok(vec![]);
                }
                let cur = *self.selection.borrow().get(&chain).unwrap_or(&0);
                let left = (cur as i32 + delta).rem_euclid(n as i32) as usize;
                self.selection.borrow_mut().insert(chain.clone(), left);
                Ok(vec![Event::BlockSelectionChanged { chain, left }])
            }
            Command::ToggleSelectedBlock { chain, side } => {
                let left = *self.selection.borrow().get(&chain).unwrap_or(&0);
                let idx = match side {
                    PairSide::Left => left,
                    PairSide::Right => left + 1,
                };
                let mut proj = self.project.borrow_mut();
                let Some(c) = proj.chains.iter_mut().find(|c| c.id == chain) else {
                    return Ok(vec![]);
                };
                // Side past the end of the chain ⇒ no-op (odd block count
                // or pair at the tail).
                let Some(b) = c.blocks.get_mut(idx) else {
                    return Ok(vec![]);
                };
                b.enabled = !b.enabled;
                Ok(vec![Event::BlockEnabledChanged {
                    chain: chain.clone(),
                    block: b.id.clone(),
                    enabled: b.enabled,
                }])
            }
            other => unreachable!("handle_block_selection got {other:?}"),
        }
    }
}
