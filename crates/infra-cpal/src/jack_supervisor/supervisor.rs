//! Responsibility: owns the state of every jackd server openrig controls.
//!
//! The supervisor drives the [`JackServerState`] state machine with calls to a
//! [`JackBackend`] implementation. Tests substitute [`MockBackend`] for
//! deterministic exercises of the transitions; in production the
//! `LiveJackBackend` performs real `jackd` spawns and libjack probes.
//!
//! What the supervisor DOES with that state lives one job per sibling file:
//! `supervisor_ensure` (bring a server up), `supervisor_spawn` (retry loop),
//! `supervisor_config_delta` (restart vs live buffer resize),
//! `supervisor_shutdown`, `supervisor_health`, `supervisor_events`.
//!
//! `dead_code` is allowed at the file level — `register_client`,
//! `unregister_client`, `events` and related APIs are part of the observable
//! surface tested via MockBackend but not yet consumed by a production path.

#![allow(dead_code)]
//!
//! Invariants the supervisor enforces (none of these can be bypassed by the
//! backend or the caller):
//!
//! 1. The pre-restart teardown hook runs BEFORE `terminate` whenever the
//!    transition destroys a previously-`Ready` server. This is the only way
//!    callers can drop their `AsyncClient` handles before the jackd they
//!    reference disappears.
//! 2. `spawn` → `post_ready_status` must return `Healthy` before the
//!    supervisor emits `ServerReady`. A vanished socket or driver failure
//!    leaves the server in `Failed` with retry metadata.
//! 3. `shutdown_all` and `stop_server` always call `forget` on the backend,
//!    so no PIDs, caches or reaper handles survive a stopped server.
//! 4. `health_check` is non-destructive — it only records a verdict; actual
//!    restarts happen on the next `ensure_server`.

use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::backend::{JackBackend, PostReadyStatus};
use super::types::{
    HealthStatus, JackConfig, JackMeta, JackServerState, RestartReason, ServerName, SupervisorEvent,
};

/// Per-server state kept inside the supervisor. The backend owns the
/// process-level resources (Child, reaper thread, cached connections); this
/// struct only records what the state machine needs to decide transitions.
pub(super) struct JackServer {
    pub(super) name: ServerName,
    pub(super) state: JackServerState,
    /// Number of client handles registered against this server. Used by
    /// `ensure_server` to skip the pre-restart teardown hook when nobody is
    /// actually holding an `AsyncClient` — restart is still safe; the hook
    /// just isn't needed.
    pub(super) client_count: usize,
    /// Last health verdict recorded by `health_check`. `None` means no check
    /// has run since the server reached `Ready`.
    pub(super) last_health: Option<HealthStatus>,
}

impl JackServer {
    pub(super) fn new(name: ServerName) -> Self {
        Self {
            name,
            state: JackServerState::NotStarted,
            client_count: 0,
            last_health: None,
        }
    }
}

/// The supervisor is parameterized over the backend type so both the live
/// impl and tests avoid a `Box<dyn JackBackend>` indirection. Callers that
/// must work with multiple backend types at runtime can wrap this in
/// `enum RuntimeBackend { Live(...), Mock(...) }` and dispatch themselves.
pub struct JackSupervisor<B: JackBackend> {
    pub(super) backend: B,
    pub(super) servers: HashMap<ServerName, JackServer>,
    pub(super) subscribers: Mutex<Vec<Sender<SupervisorEvent>>>,
}

impl<B: JackBackend> JackSupervisor<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            servers: HashMap::new(),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Record that a new libjack client was opened against `name`. The
    /// supervisor uses the count to decide whether the teardown hook needs to
    /// run on the next restart. Caller guarantees: every `register_client`
    /// is paired with exactly one `unregister_client`.
    pub fn register_client(&mut self, name: &ServerName) {
        if let Some(s) = self.servers.get_mut(name) {
            s.client_count += 1;
        }
    }

    /// Pair to `register_client`. Saturating — extra `unregister_client` calls
    /// are a no-op rather than a panic so drop impls can be defensive.
    pub fn unregister_client(&mut self, name: &ServerName) {
        if let Some(s) = self.servers.get_mut(name) {
            if s.client_count > 0 {
                s.client_count -= 1;
            }
        }
    }

    /// Return cached metadata for a `Ready` server without probing. Fails if
    /// the server has never reached `Ready`.
    pub fn meta(&self, name: &ServerName) -> Result<JackMeta> {
        match self.servers.get(name).map(|s| &s.state) {
            Some(JackServerState::Ready { meta, .. }) => Ok(meta.clone()),
            Some(other) => bail!("server '{}' is not Ready (state = {:?})", name, other),
            None => bail!("unknown server '{}'", name),
        }
    }

    /// Inspect the current state of a server. Primarily for tests and logs.
    pub fn state(&self, name: &ServerName) -> Option<&JackServerState> {
        self.servers.get(name).map(|s| &s.state)
    }

    /// Subscribe to the event stream. Each caller gets its own receiver; the
    /// supervisor fan-outs on every emit.
    pub fn events(&self) -> Receiver<SupervisorEvent> {
        let (tx, rx) = channel();
        self.subscribers.lock().unwrap().push(tx);
        rx
    }

    /// Number of currently-registered clients for `name`. Test-only.
    #[cfg(test)]
    pub fn client_count(&self, name: &ServerName) -> usize {
        self.servers.get(name).map(|s| s.client_count).unwrap_or(0)
    }

    pub(super) fn set_state(&mut self, name: &ServerName, new_state: JackServerState) {
        if let Some(s) = self.servers.get_mut(name) {
            s.state = new_state;
        }
    }
}

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;
