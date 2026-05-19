//! #436 Grupo D-4: editar parâmetro de bloco na janela Block Editor
//! (always-open) deve ir por `Command::SetBlockParameter*` /
//! `PickBlockParameterFile` — não `schedule_*persist` sem Command.
//! Funções puras testáveis; a closure Slint só renderiza + chama estas.
//!
//! Só para bloco JÁ existente (`block_index` resolvido). Atualização
//! visual imediata do item (set_block_parameter_*) continua na closure
//! (render). Aqui é só a mutação de estado, via dispatcher compartilhado.

use std::path::PathBuf;

use anyhow::Result;

use application::command::Command;
use application::dispatcher::CommandDispatcher;

use crate::state::ProjectSession;

fn ids(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
) -> Result<(domain::ids::ChainId, domain::ids::BlockId)> {
    let proj = session.project.borrow();
    let chain = proj
        .chains
        .get(chain_index)
        .ok_or_else(|| anyhow::anyhow!("chain index {chain_index} out of range"))?;
    let block = chain
        .blocks
        .get(block_index)
        .ok_or_else(|| anyhow::anyhow!("block index {block_index} out of range"))?;
    Ok((chain.id.clone(), block.id.clone()))
}

/// `Command::SetBlockParameterNumber`.
pub(crate) fn apply_set_block_parameter_number(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
    path: &str,
    value: f32,
) -> Result<()> {
    let (chain, block) = ids(session, chain_index, block_index)?;
    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain,
            block,
            path: path.to_string(),
            value: value as f64,
        })?;
    Ok(())
}

/// `Command::SetBlockParameterBool`.
pub(crate) fn apply_set_block_parameter_bool(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
    path: &str,
    value: bool,
) -> Result<()> {
    let (chain, block) = ids(session, chain_index, block_index)?;
    session.dispatcher.dispatch(Command::SetBlockParameterBool {
        chain,
        block,
        path: path.to_string(),
        value,
    })?;
    Ok(())
}

/// `Command::SetBlockParameterText`.
pub(crate) fn apply_set_block_parameter_text(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
    path: &str,
    value: &str,
) -> Result<()> {
    let (chain, block) = ids(session, chain_index, block_index)?;
    session.dispatcher.dispatch(Command::SetBlockParameterText {
        chain,
        block,
        path: path.to_string(),
        value: value.to_string(),
    })?;
    Ok(())
}

/// `Command::SelectBlockParameterOption`.
pub(crate) fn apply_select_block_parameter_option(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
    path: &str,
    value: &str,
    index: usize,
) -> Result<()> {
    let (chain, block) = ids(session, chain_index, block_index)?;
    session
        .dispatcher
        .dispatch(Command::SelectBlockParameterOption {
            chain,
            block,
            path: path.to_string(),
            value: value.to_string(),
            index,
        })?;
    Ok(())
}

/// `Command::PickBlockParameterFile`.
pub(crate) fn apply_pick_block_parameter_file(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
    path: &str,
    file: PathBuf,
) -> Result<()> {
    let (chain, block) = ids(session, chain_index, block_index)?;
    session.dispatcher.dispatch(Command::PickBlockParameterFile {
        chain,
        block,
        path: path.to_string(),
        file,
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "block_window_param_apply_tests.rs"]
mod tests;
