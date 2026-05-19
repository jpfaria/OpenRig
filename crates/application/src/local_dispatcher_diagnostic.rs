//! #436 H — `Command::SetTunerEnabled` / `SetSpectrumEnabled`: ligar/
//! desligar o tuner/spectrum (analisadores) é negócio (runtime). O
//! adapter constrói/derruba a sessão de análise + timer + runtime
//! (`wire_power`); o dispatcher registra a intenção e sinaliza via
//! evento, pra MCP/MIDI/GUI pedirem pela mesma porta (precedente
//! `SaveProject`). File-per-feature.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::SetTunerEnabled` / `SetSpectrumEnabled` — registra a
    /// intenção e sinaliza o evento. O build/teardown da sessão de
    /// análise é do adapter (precedente `SaveProject`).
    pub(crate) fn handle_diagnostic_enabled(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetTunerEnabled { enabled } => {
                Ok(vec![Event::TunerEnabledChanged { enabled }])
            }
            Command::SetSpectrumEnabled { enabled } => {
                Ok(vec![Event::SpectrumEnabledChanged { enabled }])
            }
            other => {
                unreachable!("handle_diagnostic_enabled received non-diagnostic command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_diagnostic_tests.rs"]
mod tests;
