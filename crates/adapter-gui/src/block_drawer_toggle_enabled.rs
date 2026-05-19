//! #436 Grupo D-3: alternar enabled de um bloco no Block Editor/drawer
//! deve ir por `Command::ToggleBlockEnabled` — não mutar o draft e
//! persistir na GUI. Função pura testável; a closure Slint só a chama.
//!
//! Só para bloco JÁ existente (`block_index` resolvido). Bloco novo no
//! picker tem `enabled` só no draft até `AddBlock` no save — fora daqui.

use anyhow::{anyhow, Result};

use application::command::Command;
use application::dispatcher::CommandDispatcher;

use crate::state::ProjectSession;

/// Resolve o bloco em `(chain_index, block_index)` e despacha
/// `Command::ToggleBlockEnabled` no dispatcher compartilhado.
pub(crate) fn apply_toggle_block_drawer_enabled(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
) -> Result<()> {
    let (chain_id, block_id) = {
        let proj = session.project.borrow();
        let chain = proj
            .chains
            .get(chain_index)
            .ok_or_else(|| anyhow!("chain index {chain_index} out of range"))?;
        let block = chain
            .blocks
            .get(block_index)
            .ok_or_else(|| anyhow!("block index {block_index} out of range"))?;
        (chain.id.clone(), block.id.clone())
    };
    session.dispatcher.dispatch(Command::ToggleBlockEnabled {
        chain: chain_id,
        block: block_id,
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "block_drawer_toggle_enabled_tests.rs"]
mod tests;
