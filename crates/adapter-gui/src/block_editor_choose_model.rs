//! #436 Grupo D-1: escolher modelo no Block Editor (janela always-open /
//! drawer) deve ir por `Command::ReplaceBlockModel` — não mutar o draft
//! na GUI. Lógica de negócio extraída pra esta função pura, testável
//! in-memory; a closure Slint só a chama.
//!
//! Só atua sobre bloco JÁ existente (`block_index` resolvido). Bloco novo
//! no picker é preview de draft até `AddBlock` no save — fora daqui.

use anyhow::{anyhow, Result};

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use project::block::AudioBlockKind;

use crate::project_view::block_model_picker_items;
use crate::state::ProjectSession;

/// Resolve o bloco em `(chain_index, block_index)`, mapeia `model_index`
/// pelo mesmo picker que a GUI usa, e despacha `Command::ReplaceBlockModel`
/// no dispatcher compartilhado — uma fonte só (GUI/MIDI/MCP).
pub(crate) fn apply_choose_block_model(
    session: &ProjectSession,
    chain_index: usize,
    block_index: usize,
    model_index: usize,
) -> Result<()> {
    // Resolve ids + effect_type/instrument numa borrow escopada; nunca
    // segurar a borrow através do dispatch (padrão chain_row_wiring).
    let (chain_id, block_id, effect_type, instrument) = {
        let proj = session.project.borrow();
        let chain = proj
            .chains
            .get(chain_index)
            .ok_or_else(|| anyhow!("chain index {chain_index} out of range"))?;
        let block = chain
            .blocks
            .get(block_index)
            .ok_or_else(|| anyhow!("block index {block_index} out of range"))?;
        let effect_type = match &block.kind {
            AudioBlockKind::Core(cb) => cb.effect_type.clone(),
            other => {
                return Err(anyhow!(
                    "block {:?} is not a model-bearing Core block: {other:?}",
                    block.id
                ))
            }
        };
        (
            chain.id.clone(),
            block.id.clone(),
            effect_type,
            chain.instrument.clone(),
        )
    };
    // Mesmo picker que a GUI usa: índice → model_id.
    let items = block_model_picker_items(&effect_type, &instrument);
    let model_id = items
        .get(model_index)
        .ok_or_else(|| anyhow!("model index {model_index} out of range"))?
        .model_id
        .to_string();
    // Negócio no dispatcher — fonte única (GUI/MIDI/MCP).
    session.dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: chain_id,
        block: block_id,
        model_id,
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "block_editor_choose_model_tests.rs"]
mod tests;
