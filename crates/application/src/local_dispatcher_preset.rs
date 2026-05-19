//! #436 F — `Command::SaveChainPreset` / `Command::DeleteChainPreset`:
//! salvar/apagar um preset de chain é negócio (arquivo de preset).
//! Precedente `SaveProject`: o adapter faz o I/O de arquivo; o
//! dispatcher registra a intenção e sinaliza via evento, pra MCP/MIDI/
//! GUI pedirem pela mesma porta. File-per-feature.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::SaveChainPreset` / `DeleteChainPreset` — registra a
    /// intenção e sinaliza o evento. O I/O de arquivo é do adapter
    /// (precedente `SaveProject`).
    pub(crate) fn handle_chain_preset(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SaveChainPreset { name } => Ok(vec![Event::ChainPresetSaved { name }]),
            Command::DeleteChainPreset { name } => Ok(vec![Event::ChainPresetDeleted { name }]),
            other => {
                unreachable!("handle_chain_preset received non-preset command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_preset_tests.rs"]
mod tests;
