//! #436 E — `Command::CloseProject`: voltar ao launcher (fechar o
//! projeto, parar runtime, soltar a sessão) é negócio. Precedente
//! `SaveProject`: o adapter faz o teardown de runtime/sessão; o
//! dispatcher registra a intenção e sinaliza via
//! `Event::ProjectClosed`, pra MCP/MIDI/GUI pela mesma porta.
//! File-per-feature.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::CloseProject` — registra a intenção e sinaliza
    /// `Event::ProjectClosed`. O teardown de runtime + drop da sessão
    /// é do adapter (precedente `SaveProject`).
    pub(crate) fn handle_close_project(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::CloseProject => Ok(vec![Event::ProjectClosed]),
            other => {
                unreachable!("handle_close_project received non-close command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_close_tests.rs"]
mod tests;
