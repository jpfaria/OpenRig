//! #436 F — `Command::SetLanguage`: trocar idioma é negócio (estado de
//! preferência). Segue o precedente `SaveProject`: o adapter faz a
//! persistência/efeito (FilesystemStorage + i18n live swap); o Command
//! existe pra MCP/MIDI poderem pedir a troca e registra a intenção via
//! evento. Handler em arquivo próprio (file-per-feature).

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::SetLanguage` — records the language-change intent and
    /// signals it via `Event::LanguageChanged`. The actual persistence
    /// + live i18n swap is the adapter's job (the `SaveProject`
    /// precedent: "File I/O happens in the adapter ... the dispatcher
    /// signals completion via events only").
    pub(crate) fn handle_set_language(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::SetLanguage { language } => Ok(vec![Event::LanguageChanged { language }]),
            other => unreachable!("handle_set_language received non-language command: {other:?}"),
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_language_tests.rs"]
mod tests;
