//! #436 F — `Command::RemoveRecentProject`: remover um projeto recente
//! é negócio (preferência persistida em app-config). Precedente
//! `SaveProject`: o adapter faz a persistência (`save_app_config`); o
//! dispatcher registra a intenção e sinaliza via
//! `Event::RecentProjectRemoved`. File-per-feature.

use anyhow::Result;

use crate::command::Command;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `Command::RemoveRecentProject` — registra a intenção e sinaliza
    /// `Event::RecentProjectRemoved { index }`. A mutação/persistência
    /// do app-config é do adapter (precedente `SaveProject`).
    pub(crate) fn handle_remove_recent_project(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::RemoveRecentProject { index } => {
                Ok(vec![Event::RecentProjectRemoved { index }])
            }
            other => {
                unreachable!("handle_remove_recent_project received non-recent command: {other:?}")
            }
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_recent_tests.rs"]
mod tests;
