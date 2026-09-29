//! Responsibility: borrows a chain out of the dispatcher state.
//! `LocalDispatcher` chain/block borrow helpers (issue #792 split).
//!
//! Single responsibility: the shared chain-not-found / block-not-found lookup
//! that every block- and chain-scoped `handle_*` module reuses. No command
//! handling of its own.

use anyhow::Result;

use domain::ids::{BlockId, ChainId};
use project::block::find_block_mut;

use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// Borrow the project mutably, locate `chain` then `block`, and run `f`
    /// against the located block. Centralises the chain-not-found /
    /// block-not-found lookup that every block-scoped arm performed inline.
    ///
    /// `pub(crate)` so the per-feature `handle_*` modules
    /// (`local_dispatcher_block_*`, `local_dispatcher_chain_*`) can share it.
    ///
    /// #328: the block is found at the top level or inside a split path
    /// (`project::block::find_block_mut`), so every block-scoped command
    /// reaches a path block by its id. The error strings are unchanged.
    pub(crate) fn with_block<R>(
        &self,
        chain: &ChainId,
        block: &BlockId,
        f: impl FnOnce(&mut project::block::AudioBlock) -> Result<R>,
    ) -> Result<R> {
        let mut proj = self.project.borrow_mut();
        let Some(target_chain) = proj.chains.iter_mut().find(|c| c.id == *chain) else {
            return Err(anyhow::anyhow!("chain not found: {:?}", chain));
        };
        let Some(target_block) = find_block_mut(&mut target_chain.blocks, &block.0) else {
            return Err(anyhow::anyhow!("block not found: {:?}", block));
        };
        f(target_block)
    }

    /// Borrow the project mutably, locate `chain`, and run `f` against it.
    /// Centralises the chain-not-found lookup shared by chain-scoped arms.
    ///
    /// `pub(crate)` so the per-feature `handle_*` modules can share it.
    ///
    /// Behaviour is byte-identical to the previous inline form: same
    /// `borrow_mut` scope, same `find` order, same error string.
    pub(crate) fn with_chain<R>(
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
}
