//! Responsibility: stops the jackd servers the supervisor owns.
//!
//! Invariant 3: both doors call `forget` on the backend, so no PIDs, caches
//! or reaper handles survive a stopped server.

use anyhow::Result;

use super::backend::JackBackend;
use super::supervisor::JackSupervisor;
use super::types::{JackServerState, ServerName, SupervisorEvent};

impl<B: JackBackend> JackSupervisor<B> {
    /// Stop a server cleanly. No-op when the server is `NotStarted` or
    /// `Failed`. Caller is responsible for dropping their `AsyncClient`s
    /// before calling; `stop_server` does *not* fire the teardown hook.
    pub fn stop_server(&mut self, name: &ServerName) -> Result<()> {
        let should_stop = self
            .servers
            .get(name)
            .map(|s| !s.state.is_terminal())
            .unwrap_or(false);
        if !should_stop {
            return Ok(());
        }
        self.backend.terminate(name)?;
        self.backend.forget(name);
        self.set_state(name, JackServerState::NotStarted);
        if let Some(s) = self.servers.get_mut(name) {
            s.client_count = 0;
            s.last_health = None;
        }
        self.emit(SupervisorEvent::ServerStopped { name: name.clone() });
        Ok(())
    }

    /// Stop every non-terminal server. Idempotent — calling twice after a
    /// stop returns `Ok(())` with no backend calls.
    pub fn shutdown_all(&mut self) -> Result<()> {
        let names: Vec<ServerName> = self.servers.keys().cloned().collect();
        let mut first_error: Option<anyhow::Error> = None;
        for name in names {
            if let Err(e) = self.stop_server(&name) {
                log::warn!("shutdown_all: failed to stop '{}': {}", name, e);
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
        match first_error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}
