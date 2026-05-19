//! #436 G — `Command::SetOutputMuted`: silenciar/dessilenciar a saída
//! (tuner mute) é negócio (runtime). Segue o precedente `SaveProject`:
//! o adapter aplica no runtime (`rt.set_output_muted`); o dispatcher
//! registra a intenção e sinaliza via `Event::OutputMutedChanged`, de
//! forma que MCP/MIDI/GUI pedem pela mesma porta. File-per-feature.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::SetOutputMuted` — registra o estado de mute e sinaliza
    /// via `Event::OutputMutedChanged`. O efeito real no runtime de
    /// áudio é do adapter (precedente `SaveProject`).
    pub(crate) fn handle_set_output_muted(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetOutputMuted { muted } => Ok(vec![Event::OutputMutedChanged { muted }]),
            other => {
                unreachable!("handle_set_output_muted received non-mute command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_output_tests.rs"]
mod tests;
