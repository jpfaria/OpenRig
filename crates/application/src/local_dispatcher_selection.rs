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

    /// `Command::SelectActiveBlockRelative { delta }`. Cycles through
    /// the active chain's `blocks` list (wraps both ways). No-op when
    /// no chain is active or the chain has no blocks.
    pub(crate) fn handle_select_active_block_relative(&self, delta: i32) -> Result<Vec<Event>> {
        let project = self.project.borrow();
        let mut sel = self.selection_state.write().expect("selection state poisoned");
        let Some(active_chain_id) = sel.active_chain.clone() else {
            return Ok(vec![]);
        };
        let Some(chain) = project.chains.iter().find(|c| c.id.0 == active_chain_id) else {
            return Ok(vec![]);
        };
        if chain.blocks.is_empty() {
            return Ok(vec![]);
        }
        let n = chain.blocks.len() as i32;
        let current_idx = sel
            .active_block
            .as_deref()
            .and_then(|id| chain.blocks.iter().position(|b| b.id.0 == id));
        let next_idx = match current_idx {
            None => 0,
            Some(i) => (((i as i32 + delta) % n) + n) as usize % n as usize,
        };
        sel.active_block = Some(chain.blocks[next_idx].id.0.clone());
        sel.active_block_enabled = chain.blocks[next_idx].enabled;
        drop(sel);

        // Seed the legacy per-chain selection so the existing GUI's
        // "selected block" indicator follows MIDI navigation.
        let chain_id = ChainId(active_chain_id);
        self.selection
            .borrow_mut()
            .insert(chain_id, next_idx);

        Ok(vec![Event::ProjectMutated])
    }
}
