//! #436 G — `SelectionCommand::SetOutputMuted`: silenciar/dessilenciar a saída
//! (tuner mute) é negócio (runtime). Segue o precedente `SaveProject`:
//! o adapter aplica no runtime (`rt.set_output_muted`); o dispatcher
//! registra a intenção e sinaliza via `Event::OutputMutedChanged`, de
//! forma que MCP/MIDI/GUI pedem pela mesma porta. File-per-feature.

use anyhow::Result;

use crate::command::{Command, SelectionCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `SelectionCommand::SetOutputMuted` — registra o estado de mute e sinaliza
    /// via `Event::OutputMutedChanged`. O efeito real no runtime de
    /// áudio é do adapter (precedente `SaveProject`).
    pub(crate) fn handle_set_output_muted(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Selection(SelectionCommand::SetOutputMuted { muted }) => {
                // #548: mirror into SelectionState so MIDI slot
                // `toggle_output_mute` reads the current state.
                if let Ok(mut s) = self.selection_state.write() {
                    s.output_muted = muted;
                }
                Ok(vec![Event::OutputMutedChanged { muted }])
            }
            other => {
                unreachable!("handle_set_output_muted received non-mute command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_output_tests.rs"]
mod tests;
