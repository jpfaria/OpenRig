//! #548: handlers for the 3 MIDI selection commands.
//!
//! Operate on `LocalDispatcher::selection_state` (`SelectionState`) using
//! the project's chain list / block list to resolve "the previous /
//! next one". Wrap on both ends — a footswitch has no edges.

use anyhow::Result;

use domain::ids::ChainId;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::SelectActiveChainRelative { delta }`. Cycles through
    /// `project.chains` (wraps both ways). Clears `active_block` when
    /// the active chain changes — a block lives inside one chain.
    /// No-op when the project has no chains.
    pub(crate) fn handle_select_active_chain_relative(&self, delta: i32) -> Result<Vec<Event>> {
        let project = self.project.borrow();
        if project.chains.is_empty() {
            return Ok(vec![]);
        }
        let n = project.chains.len() as i32;
        let mut sel = self.selection_state.write().expect("selection state poisoned");
        let current_idx = sel
            .active_chain
            .as_deref()
            .and_then(|id| project.chains.iter().position(|c| c.id.0 == id));
        let next_idx = match current_idx {
            // Seed to the first chain on the first nav from an empty state.
            None => 0,
            Some(i) => (((i as i32 + delta) % n) + n) as usize % n as usize,
        };
        let next_id = project.chains[next_idx].id.0.clone();
        let changed = sel.active_chain.as_deref() != Some(next_id.as_str());
        let new_chain_id = ChainId(next_id.clone());
        sel.active_chain_enabled = project.chains[next_idx].enabled;
        sel.active_chain = Some(next_id);
        if changed {
            sel.active_block = None;
        }
        // Drop the write guard before touching `self.selection` to avoid
        // holding two RefCell/RwLock guards at once.
        drop(sel);

        // Seed the legacy per-chain block-selection map. The existing
        // GUI uses it to render the "current chain" highlight, so this
        // is how a MIDI footswitch lights up the chain on screen.
        self.selection
            .borrow_mut()
            .entry(new_chain_id)
            .or_insert(0);

        Ok(vec![Event::ProjectMutated])
    }

    /// `Command::ToggleActiveBlockNeighborEnabled`. Flips the
    /// `enabled` flag of the block immediately AFTER `active_block` in
    /// the active chain (wraps to first). No-op when no chain/block is
    /// active or the chain has fewer than 2 blocks.
    pub(crate) fn handle_toggle_active_block_neighbor_enabled(&self) -> Result<Vec<Event>> {
        let chain_id_str = {
            let sel = self.selection_state.read().expect("selection state poisoned");
            sel.active_chain.clone()
        };
        let Some(chain_id) = chain_id_str else {
            return Ok(vec![]);
        };
        let active_block_id = {
            let sel = self.selection_state.read().expect("selection state poisoned");
            sel.active_block.clone()
        };
        let Some(active_block_id) = active_block_id else {
            return Ok(vec![]);
        };

        // Resolve the neighbor id under a brief project borrow.
        let neighbor_id = {
            let proj = self.project.borrow();
            let chain = proj.chains.iter().find(|c| c.id.0 == chain_id);
            let Some(chain) = chain else {
                return Ok(vec![]);
            };
            if chain.blocks.len() < 2 {
                return Ok(vec![]);
            }
            let active_idx = chain
                .blocks
                .iter()
                .position(|b| b.id.0 == active_block_id);
            let Some(idx) = active_idx else {
                return Ok(vec![]);
            };
            let neighbor_idx = (idx + 1) % chain.blocks.len();
            chain.blocks[neighbor_idx].id.0.clone()
        };

        // Recurse via the normal dispatch so all side effects (event
        // emission, SelectionState mirror on the active block) run
        // exactly the same way a click on that block's toggle would.
        use crate::command::{BlockId, Command};
        use crate::dispatcher::CommandDispatcher;
        self.dispatch(Command::ToggleBlockEnabled {
            chain: ChainId(chain_id),
            block: BlockId(neighbor_id),
        })
    }

    /// `Command::SelectActiveBlockRelative { delta }`. Cycles through
    /// the active chain's audio blocks — skipping Input/Output blocks
    /// that the user doesn't see on the Chains screen — wrapping both
    /// ways. No-op when no chain is active or the chain has no audio
    /// blocks.
    pub(crate) fn handle_select_active_block_relative(&self, delta: i32) -> Result<Vec<Event>> {
        use project::block::AudioBlockKind;
        let project = self.project.borrow();
        let mut sel = self.selection_state.write().expect("selection state poisoned");
        let Some(active_chain_id) = sel.active_chain.clone() else {
            return Ok(vec![]);
        };
        let Some(chain) = project.chains.iter().find(|c| c.id.0 == active_chain_id) else {
            return Ok(vec![]);
        };

        // Build the navigable view: keep the original block index but
        // drop Input/Output/Insert wrappers — those don't show as
        // "blocks" on the chain UI, so a MIDI step that lands on them
        // would look like a no-op to the user.
        let navigable: Vec<(usize, &project::block::AudioBlock)> = chain
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                !matches!(
                    b.kind,
                    AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
                )
            })
            .collect();

        if navigable.is_empty() {
            return Ok(vec![]);
        }
        let n = navigable.len() as i32;
        let current_pos_in_navigable = sel
            .active_block
            .as_deref()
            .and_then(|id| navigable.iter().position(|(_, b)| b.id.0 == id));
        let next_pos = match current_pos_in_navigable {
            None => 0,
            Some(i) => (((i as i32 + delta) % n) + n) as usize % n as usize,
        };
        let (real_idx, next_block) = navigable[next_pos];
        sel.active_block = Some(next_block.id.0.clone());
        sel.active_block_enabled = next_block.enabled;
        drop(sel);

        // Seed the legacy per-chain selection (uses the REAL index in
        // the full blocks list — the GUI's compact view indexes there).
        let chain_id = ChainId(active_chain_id);
        self.selection
            .borrow_mut()
            .insert(chain_id, real_idx);

        Ok(vec![Event::ProjectMutated])
    }
}
