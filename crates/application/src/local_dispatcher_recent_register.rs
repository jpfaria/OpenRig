//! #436 (sweep): registrar/invalidar projeto recente é negócio (config
//! persistida = estado). `RemoveRecentProject` já existia; faltava o
//! lado adicionar/marcar-inválido — feito por mutação direta de
//! app_config nas closures (resíduo achado na varredura). Precedente
//! `SaveProject`: o adapter faz a persistência (`save_app_config`); o
//! dispatcher registra a intenção e sinaliza via evento.

use anyhow::Result;

use crate::command::{Command, ProjectCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `ProjectCommand::RegisterRecentProject` / `MarkRecentProjectInvalid` —
    /// registra a intenção e sinaliza o evento. A mutação/persistência
    /// do app-config é do adapter (precedente `SaveProject`).
    pub(crate) fn handle_recent_register(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Project(ProjectCommand::RegisterRecentProject { path, name }) => {
                Ok(vec![Event::RecentProjectRegistered { path, name }])
            }
            Command::Project(ProjectCommand::MarkRecentProjectInvalid { path, reason }) => {
                Ok(vec![Event::RecentProjectInvalidated { path, reason }])
            }
            other => unreachable!(
                "handle_recent_register received non-recent-register command: {other:?}"
            ),
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_recent_register_tests.rs"]
mod tests;
