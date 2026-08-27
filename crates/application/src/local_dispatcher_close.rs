//! Responsibility: handles the close commands.
//! #436 E — `ProjectCommand::CloseProject`: voltar ao launcher (fechar o
//! projeto, parar runtime, soltar a sessão) é negócio. Precedente
//! `SaveProject`: o adapter faz o teardown de runtime/sessão; o
//! dispatcher registra a intenção e sinaliza via
//! `Event::ProjectClosed`, pra MCP/MIDI/GUI pela mesma porta.
//! File-per-feature.

use anyhow::Result;

use crate::command::{Command, ProjectCommand};
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    /// `ProjectCommand::CloseProject` / `StopProjectRuntime` — stop the rig,
    /// and for the close also signal `Event::ProjectClosed`.
    ///
    /// #127: the runtime teardown is applied HERE now, through
    /// [`crate::runtime_control::RuntimeControl::stop_project_runtime`]. It
    /// used to be a GUI function call made right after this command was
    /// dispatched, so a `CloseProject` issued over MCP/gRPC left every stream
    /// open and sounding — and a remote client that had started the rig had no
    /// way at all to stop it. Dropping the session itself stays with the
    /// adapter (`SaveProject` precedent): only the AUDIO moved onto the bus.
    ///
    /// `StopProjectRuntime` emits nothing, for the same reason
    /// `SyncChainRuntime` does: it changes no project state.
    pub(crate) fn handle_close_project(&self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::Project(ProjectCommand::CloseProject) => {
                self.stop_the_rig();
                Ok(vec![Event::ProjectClosed])
            }
            Command::Project(ProjectCommand::StopProjectRuntime) => {
                self.stop_the_rig();
                Ok(vec![])
            }
            other => {
                unreachable!("handle_close_project received non-close command: {other:?}")
            }
        }
    }

    /// Ask the attached frontend to drop every stream it has open. A transport
    /// that hosts no audio has nothing attached and this is a no-op — the
    /// command still succeeds, exactly like every other runtime door.
    fn stop_the_rig(&self) {
        if let Some(control) = self.runtime_control() {
            control.stop_project_runtime();
        }
    }
}

#[cfg(test)]
#[path = "local_dispatcher_close_tests.rs"]
mod tests;

// #127 (Task 14): the three runtime doors this task built — stop the rig,
// rebuild the whole graph, drop a deleted chain's stream. They are applied by
// handlers in three different modules (here, `local_dispatcher_project`,
// `local_dispatcher_chain_crud`) but they are one story, so they are tested
// together and hang off the one that owns the stop.
#[cfg(test)]
#[path = "local_dispatcher_runtime_doors_tests.rs"]
mod runtime_doors_tests;
