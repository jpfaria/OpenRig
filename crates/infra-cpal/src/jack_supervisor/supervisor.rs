//! `JackSupervisor` — single owner of every `jackd` server openrig controls.
//!
//! The supervisor drives the [`JackServerState`] state machine with calls to a
//! [`JackBackend`] implementation. Tests substitute [`MockBackend`] for
//! deterministic exercises of the transitions. In production the
//! `LiveJackBackend` performs real `jackd` spawns and libjack probes.
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
    HealthStatus, JackConfig, JackMeta, JackServerState, RestartReason, ServerName,
    SupervisorEvent,
};

/// Maximum number of spawn attempts per `ensure_server` call.
const MAX_SPAWN_ATTEMPTS: u32 = 3;

/// Wall-clock delay between spawn retries.
const SPAWN_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Upper bound on the buffer-size fallback growth. `buf=64` that trips
/// "Broken pipe" gets bumped to 128, then 256, then 512, then 1024 — beyond
/// that we declare defeat and fail.
const MAX_BUFFER_CLAMP: u32 = 1024;

/// Per-server state kept inside the supervisor. The backend owns the
/// process-level resources (Child, reaper thread, cached connections); this
/// struct only records what the state machine needs to decide transitions.
struct JackServer {
    name: ServerName,
    state: JackServerState,
    /// Number of client handles registered against this server. Used by
    /// `ensure_server` to skip the pre-restart teardown hook when nobody is
    /// actually holding an `AsyncClient` — restart is still safe; the hook
    /// just isn't needed.
    client_count: usize,
    /// Last health verdict recorded by `health_check`. `None` means no check
    /// has run since the server reached `Ready`.
    last_health: Option<HealthStatus>,
}

impl JackServer {
    fn new(name: ServerName) -> Self {
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
    backend: B,
    servers: HashMap<ServerName, JackServer>,
    subscribers: Mutex<Vec<Sender<SupervisorEvent>>>,
}

impl<B: JackBackend> JackSupervisor<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            servers: HashMap::new(),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Ensure that `name` is in [`JackServerState::Ready`] with a configuration
    /// matching `desired`. Behavior depends on the current state:
    ///
    /// - `NotStarted` or `Failed` → fresh spawn with retry + buffer fallback.
    /// - `Ready` with matching config → no-op (cached meta is returned).
    /// - `Ready` with different config → call `before_restart`, then
    ///   `terminate` + spawn loop.
    /// - `Spawning` / `Restarting` (should not be observable from outside; the
    ///   supervisor is single-threaded) → treated as hard error.
    ///
    /// `before_restart` is invoked at most once per call and only when a
    /// restart is actually triggered. Callers use it to drop all
    /// `AsyncClient`s tied to the old jackd before it receives SIGTERM.
    pub fn ensure_server(
        &mut self,
        name: &ServerName,
        desired: &JackConfig,
        before_restart: &mut dyn FnMut(&ServerName),
    ) -> Result<JackMeta> {
        if !self.servers.contains_key(name) {
            self.servers
                .insert(name.clone(), JackServer::new(name.clone()));
        }

        // Adoption path: when the supervisor state is NotStarted but a jackd
        // socket is already present for `name`, an externally-launched server
        // is running (e.g. start_jack_in_background at boot, or a previous
        // openrig controller that was recreated by the GUI). Probe the
        // running server: if the config matches `desired`, adopt it without
        // spawning; if not, terminate it cleanly (and let the spawn loop
        // relaunch under our supervision).
        //
        // Skipping this step is what caused the issue #308 hardware
        // regression "toggle chain off+on makes audio stop": the GUI
        // recreates the controller, the new supervisor had NotStarted state,
        // `spawn` cleaned up /dev/shm/jack_*_0 sockets (thinking they were
        // stale) and hosed the still-running jackd, and the retry failed
        // with "device already in use (jackd PID N)".
        if matches!(
            self.servers.get(name).map(|s| &s.state),
            Some(JackServerState::NotStarted)
        ) && self.backend.is_socket_present(name)
        {
            log::info!(
                "supervisor::ensure_server: adopting running jackd for '{}'",
                name
            );
            match self.backend.probe_meta(name) {
                Ok(meta) => {
                    if meta.sample_rate == desired.sample_rate
                        && meta.buffer_size == desired.buffer_size
                    {
                        self.set_state(
                            name,
                            JackServerState::Ready {
                                meta: meta.clone(),
                                launched_config: desired.clone(),
                                ready_at: Instant::now(),
                            },
                        );
                        if let Some(s) = self.servers.get_mut(name) {
                            s.last_health = Some(HealthStatus::Healthy);
                        }
                        self.emit(SupervisorEvent::ServerReady {
                            name: name.clone(),
                            meta: meta.clone(),
                        });
                        return Ok(meta);
                    }
                    log::info!(
                        "supervisor::ensure_server: adopted jackd config mismatch for '{}' (sr={} buf={} → sr={} buf={}), restarting",
                        name,
                        meta.sample_rate,
                        meta.buffer_size,
                        desired.sample_rate,
                        desired.buffer_size
                    );
                    let reason = RestartReason::ConfigMismatch {
                        old: JackConfig {
                            sample_rate: meta.sample_rate,
                            buffer_size: meta.buffer_size,
                            ..desired.clone()
                        },
                        new: desired.clone(),
                    };
                    // Terminate the running jackd — no teardown hook, we
                    // didn't register this server and no client is ours.
                    self.emit(SupervisorEvent::RestartRequested {
                        name: name.clone(),
                        reason,
                    });
                    let _ = self.backend.terminate(name);
                    self.backend.forget(name);
                }
                Err(e) => {
                    log::warn!(
                        "supervisor::ensure_server: socket present but probe failed for '{}' ({}), killing zombie",
                        name,
                        e
                    );
                    self.emit(SupervisorEvent::RestartRequested {
                        name: name.clone(),
                        reason: RestartReason::Zombie,
                    });
                    let _ = self.backend.terminate(name);
                    self.backend.forget(name);
                }
            }
        }

        let needs_restart = matches!(
            self.servers.get(name).map(|s| &s.state),
            Some(JackServerState::Ready { launched_config, .. })
                if launched_config != desired
        );
        if needs_restart {
            let reason = {
                let server = self.servers.get(name).expect("inserted above");
                match &server.state {
                    JackServerState::Ready { launched_config, .. } => {
                        RestartReason::ConfigMismatch {
                            old: launched_config.clone(),
                            new: desired.clone(),
                        }
                    }
                    _ => unreachable!("guarded by needs_restart matches!"),
                }
            };
            self.transition_to_restarting(name, reason, before_restart)?;
        }

        // After possible restart the server is either NotStarted / Restarting
        // (which we treat the same — both mean "needs a fresh spawn") or
        // still Ready (matching config, no restart needed).
        let server = self.servers.get(name).expect("inserted above");
        if let JackServerState::Ready { meta, .. } = &server.state {
            return Ok(meta.clone());
        }

        self.spawn_with_retries(name, desired)
    }

    /// Attempt up to [`MAX_SPAWN_ATTEMPTS`] spawns with exponential buffer
    /// fallback on post-ready driver failure. Moves the server to `Ready` on
    /// success, `Failed` on exhaustion.
    fn spawn_with_retries(&mut self, name: &ServerName, desired: &JackConfig) -> Result<JackMeta> {
        let mut attempt_config = desired.clone();
        let mut last_error: Option<String> = None;

        for attempt in 1..=MAX_SPAWN_ATTEMPTS {
            self.set_state(
                name,
                JackServerState::Spawning {
                    started_at: Instant::now(),
                    desired: attempt_config.clone(),
                },
            );
            self.emit(SupervisorEvent::ServerSpawning {
                name: name.clone(),
                config: attempt_config.clone(),
            });

            match self.backend.spawn(name, &attempt_config) {
                Ok(()) => {}
                Err(e) => {
                    last_error = Some(e.to_string());
                    self.emit(SupervisorEvent::ServerFailed {
                        name: name.clone(),
                        error: e.to_string(),
                    });
                    if attempt < MAX_SPAWN_ATTEMPTS {
                        self.backend.forget(name);
                        std::thread::sleep(SPAWN_RETRY_DELAY);
                        continue;
                    }
                    break;
                }
            }

            match self.backend.post_ready_status(name) {
                PostReadyStatus::Healthy => {}
                PostReadyStatus::SocketVanished => {
                    last_error = Some("jackd socket vanished after startup".into());
                    self.emit(SupervisorEvent::ServerDied { name: name.clone() });
                    self.backend.forget(name);
                    // Buffer was very likely too small — bump for next attempt.
                    attempt_config = bump_buffer(&attempt_config);
                    if attempt_config.buffer_size != desired.buffer_size {
                        self.emit(SupervisorEvent::BufferClampedTo {
                            name: name.clone(),
                            from: desired.buffer_size,
                            to: attempt_config.buffer_size,
                        });
                    }
                    if attempt < MAX_SPAWN_ATTEMPTS {
                        std::thread::sleep(SPAWN_RETRY_DELAY);
                        continue;
                    }
                    break;
                }
                PostReadyStatus::DriverFailure(detail) => {
                    last_error = Some(format!("ALSA/driver failure: {}", detail));
                    self.emit(SupervisorEvent::ServerDied { name: name.clone() });
                    self.backend.forget(name);
                    attempt_config = bump_buffer(&attempt_config);
                    if attempt_config.buffer_size != desired.buffer_size {
                        self.emit(SupervisorEvent::BufferClampedTo {
                            name: name.clone(),
                            from: desired.buffer_size,
                            to: attempt_config.buffer_size,
                        });
                    }
                    if attempt < MAX_SPAWN_ATTEMPTS {
                        std::thread::sleep(SPAWN_RETRY_DELAY);
                        continue;
                    }
                    break;
                }
            }

            let meta = match self.backend.probe_meta(name) {
                Ok(m) => m,
                Err(e) => {
                    last_error = Some(format!("probe_meta failed: {}", e));
                    self.backend.forget(name);
                    if attempt < MAX_SPAWN_ATTEMPTS {
                        std::thread::sleep(SPAWN_RETRY_DELAY);
                        continue;
                    }
                    break;
                }
            };

            self.set_state(
                name,
                JackServerState::Ready {
                    meta: meta.clone(),
                    launched_config: attempt_config.clone(),
                    ready_at: Instant::now(),
                },
            );
            if let Some(s) = self.servers.get_mut(name) {
                s.last_health = Some(HealthStatus::Healthy);
            }
            self.emit(SupervisorEvent::ServerReady {
                name: name.clone(),
                meta: meta.clone(),
            });
            return Ok(meta);
        }

        let error = last_error.unwrap_or_else(|| "spawn exhausted without error".into());
        self.set_state(
            name,
            JackServerState::Failed {
                last_error: error.clone(),
                attempts: MAX_SPAWN_ATTEMPTS,
            },
        );
        self.emit(SupervisorEvent::ServerFailed {
            name: name.clone(),
            error: error.clone(),
        });
        Err(anyhow!(
            "failed to bring up JACK server '{}' after {} attempts: {}",
            name,
            MAX_SPAWN_ATTEMPTS,
            error
        ))
    }

    /// Invariant-preserving transition into `Restarting`. Fires the pre-kill
    /// teardown hook when any clients are registered, emits the restart
    /// event, and calls `backend.terminate` + `backend.forget` in that order.
    fn transition_to_restarting(
        &mut self,
        name: &ServerName,
        reason: RestartReason,
        before_restart: &mut dyn FnMut(&ServerName),
    ) -> Result<()> {
        let had_clients = self
            .servers
            .get(name)
            .map(|s| s.client_count > 0)
            .unwrap_or(false);

        self.emit(SupervisorEvent::RestartRequested {
            name: name.clone(),
            reason: reason.clone(),
        });

        if had_clients {
            self.emit(SupervisorEvent::TeardownRequested { name: name.clone() });
            before_restart(name);
            if let Some(s) = self.servers.get_mut(name) {
                // Teardown contract: once the hook returns, the caller has
                // dropped every AsyncClient. We trust that and clear our
                // tracking — if the caller lied, the subsequent terminate
                // will still succeed because we SIGTERM the process itself.
                s.client_count = 0;
            }
        }

        self.set_state(name, JackServerState::Restarting { reason });
        self.backend.terminate(name)?;
        self.backend.forget(name);
        self.set_state(name, JackServerState::NotStarted);
        Ok(())
    }

    /// Side-effect-free predicate — returns true when the next
    /// `ensure_server(name, desired)` would trigger a `Ready → Restarting`
    /// transition. Callers use this to drop their `AsyncClient` handles
    /// *before* the supervisor kills jackd, preventing the libjack global
    /// state from ending up in the `ClientStatus(FAILURE | SERVER_ERROR)`
    /// limbo that bugfix/issue-294 documented.
    ///
    /// Returns false when the server is `NotStarted`, `Failed`, or
    /// `Ready` with a matching config. The latter two cases are handled
    /// in-place by `ensure_server` without needing a pre-kill teardown.
    pub fn would_restart(&self, name: &ServerName, desired: &JackConfig) -> bool {
        matches!(
            self.servers.get(name).map(|s| &s.state),
            Some(JackServerState::Ready { launched_config, .. })
                if launched_config != desired
        )
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

    /// Non-destructive check. Uses ONLY the cheap filesystem-level
    /// `is_socket_present` probe — opening a real libjack client on every
    /// health tick was observed to interfere with audio on fragile USB audio
    /// stacks (Rockchip xHCI), which is the exact pathology the supervisor
    /// exists to avoid. A missing socket flips state to `NotRunning`; a
    /// present socket is treated as `Healthy` optimistically. Zombie
    /// detection happens implicitly on the next `ensure_server` retry when
    /// a new client fails to connect.
    pub fn health_check(&mut self) -> HashMap<ServerName, HealthStatus> {
        let mut out = HashMap::new();
        let names: Vec<ServerName> = self.servers.keys().cloned().collect();
        for name in names {
            let status = self.check_one(&name);
            if let Some(s) = self.servers.get_mut(&name) {
                s.last_health = Some(status.clone());
            }
            out.insert(name, status);
        }
        out
    }

    fn check_one(&mut self, name: &ServerName) -> HealthStatus {
        let state = self.servers.get(name).map(|s| &s.state);
        match state {
            None | Some(JackServerState::NotStarted) => HealthStatus::NotRunning,
            Some(JackServerState::Failed { .. }) => HealthStatus::Failed,
            Some(JackServerState::Spawning { .. }) | Some(JackServerState::Restarting { .. }) => {
                HealthStatus::NotRunning
            }
            Some(JackServerState::Ready { .. }) => {
                if !self.backend.is_socket_present(name) {
                    HealthStatus::NotRunning
                } else {
                    HealthStatus::Healthy
                }
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

    fn set_state(&mut self, name: &ServerName, new_state: JackServerState) {
        if let Some(s) = self.servers.get_mut(name) {
            s.state = new_state;
        }
    }

    fn emit(&self, event: SupervisorEvent) {
        // Log every state transition/observation so journalctl has a
        // single-line summary of everything the supervisor did, even when
        // no subscriber is attached. On hardware the log is the primary
        // debugging channel for audio-stream incidents.
        match &event {
            SupervisorEvent::ServerSpawning { name, config } => {
                log::info!(
                    "supervisor: '{}' spawning (sr={} buf={} nperiods={})",
                    name,
                    config.sample_rate,
                    config.buffer_size,
                    config.nperiods
                );
            }
            SupervisorEvent::ServerReady { name, meta } => {
                log::info!(
                    "supervisor: '{}' ready (sr={} buf={} in={} out={})",
                    name,
                    meta.sample_rate,
                    meta.buffer_size,
                    meta.capture_port_count,
                    meta.playback_port_count
                );
            }
            SupervisorEvent::ServerFailed { name, error } => {
                log::error!("supervisor: '{}' failed: {}", name, error);
            }
            SupervisorEvent::ServerDied { name } => {
                log::warn!("supervisor: '{}' died post-ready", name);
            }
            SupervisorEvent::ServerStopped { name } => {
                log::info!("supervisor: '{}' stopped", name);
            }
            SupervisorEvent::RestartRequested { name, reason } => {
                log::info!("supervisor: '{}' restart requested ({:?})", name, reason);
            }
            SupervisorEvent::BufferClampedTo { name, from, to } => {
                log::warn!(
                    "supervisor: '{}' buffer clamped {} → {} (driver rejected requested size)",
                    name,
                    from,
                    to
                );
            }
            SupervisorEvent::TeardownRequested { name } => {
                log::info!("supervisor: '{}' teardown hook firing", name);
            }
        }
        let mut subs = self.subscribers.lock().unwrap();
        subs.retain(|tx| tx.send(event.clone()).is_ok());
    }
}

fn bump_buffer(config: &JackConfig) -> JackConfig {
    let bumped = (config.buffer_size * 2).min(MAX_BUFFER_CLAMP);
    JackConfig {
        buffer_size: bumped,
        ..config.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::backend::{JackBackend, MockBackend, MockCall, PostReadyStatus};
    use super::super::types::{
        HealthStatus, JackConfig, JackMeta, JackServerState, RestartReason, ServerName,
        SupervisorEvent,
    };
    use super::*;

    fn name() -> ServerName {
        ServerName::from("test")
    }

    fn noop_hook() -> impl FnMut(&ServerName) {
        |_: &ServerName| {}
    }

    fn make_supervisor() -> JackSupervisor<MockBackend> {
        JackSupervisor::new(MockBackend::new())
    }

    // When the supervisor is invoked against a never-seen server, the full
    // Spawn → PostReadyStatus → ProbeMeta sequence runs in order and the
    // server ends up in Ready with the probed meta.
    #[test]
    fn ensure_server_from_not_started_transitions_to_ready() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();
        let meta = sup
            .ensure_server(&name(), &config, &mut noop_hook())
            .expect("cold start succeeds");
        assert_eq!(meta.sample_rate, config.sample_rate);
        assert!(sup.state(&name()).unwrap().is_ready());

        let calls = sup.backend.calls();
        assert!(matches!(calls[0], MockCall::Spawn(_, _)));
        assert!(matches!(calls[1], MockCall::PostReadyStatus(_)));
        assert!(matches!(calls[2], MockCall::ProbeMeta(_)));
    }

    // A repeat ensure_server with the identical desired config must NOT
    // re-spawn. Cached meta is returned from the prior Ready state.
    #[test]
    fn ensure_server_with_matching_config_is_idempotent() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();
        sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        let before = sup.backend.call_count();
        sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(sup.backend.call_count(), before, "no extra backend calls");
    }

    // When the supervisor state is NotStarted but a jackd socket is already
    // present (externally launched — e.g. start_jack_in_background at boot,
    // or a previous controller whose handle was dropped without terminating
    // jackd), ensure_server must ADOPT the running server, not try to spawn
    // a new one. This is the fix for the issue #308 hardware regression
    // where toggling a chain off+on recreated the controller, the new
    // supervisor didn't know about the running jackd, and spawn tried to
    // nuke /dev/shm/jack_* sockets as "stale".
    #[test]
    fn ensure_server_adopts_running_jackd_with_matching_config() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();
        // Simulate an externally-launched jackd by seeding the mock
        // backend's running set + meta, without calling supervisor.ensure.
        sup.backend.inner.lock().unwrap().running.insert(name());
        sup.backend.set_default_meta(
            &name(),
            JackMeta {
                sample_rate: config.sample_rate,
                buffer_size: config.buffer_size,
                capture_port_count: 2,
                playback_port_count: 2,
                hw_name: "external".into(),
            },
        );

        let meta = sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.sample_rate, config.sample_rate);

        // Adoption must NOT have triggered a spawn — the backend should have
        // seen probe_meta (the adoption check) but no Spawn call.
        let calls = sup.backend.calls();
        assert!(
            calls.iter().any(|c| matches!(c, MockCall::ProbeMeta(_))),
            "adoption must probe the running server"
        );
        assert!(
            !calls.iter().any(|c| matches!(c, MockCall::Spawn(_, _))),
            "adoption must not spawn a new jackd"
        );
        assert!(
            !calls.iter().any(|c| matches!(c, MockCall::Terminate(_))),
            "adoption with matching config must not terminate"
        );
        assert!(sup.state(&name()).unwrap().is_ready());
    }

    // Adoption with mismatched config must cleanly terminate the running
    // jackd and then spawn a fresh one under supervision.
    #[test]
    fn ensure_server_adopts_and_restarts_on_config_mismatch() {
        let mut sup = make_supervisor();
        sup.backend.inner.lock().unwrap().running.insert(name());
        // External jackd running at buf=128; we want buf=256.
        sup.backend.set_default_meta(
            &name(),
            JackMeta {
                sample_rate: 48_000,
                buffer_size: 128,
                capture_port_count: 2,
                playback_port_count: 2,
                hw_name: "external".into(),
            },
        );

        let desired = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        let meta = sup.ensure_server(&name(), &desired, &mut noop_hook()).unwrap();
        assert_eq!(meta.buffer_size, 256);

        let calls = sup.backend.calls();
        let terminate_idx = calls
            .iter()
            .position(|c| matches!(c, MockCall::Terminate(_)))
            .expect("adoption mismatch must terminate");
        let spawn_idx = calls
            .iter()
            .position(|c| matches!(c, MockCall::Spawn(_, _)))
            .expect("adoption mismatch must spawn after terminate");
        assert!(terminate_idx < spawn_idx, "terminate must precede spawn");
    }

    // Full controller-recreation scenario: supervisor A spawns jackd and is
    // dropped without calling shutdown_all (the GUI path). Supervisor B is
    // created with a backend that sees the same running server and must
    // adopt it without spawning. This is the end-to-end shape of the issue
    // #308 hardware regression.
    #[test]
    fn supervisor_b_adopts_jackd_left_running_by_dropped_supervisor_a() {
        use super::super::backend::MockBackendInner;
        use std::sync::Arc as StdArc;
        let shared_inner = StdArc::new(std::sync::Mutex::new(MockBackendInner::default()));
        let backend_a = MockBackend { inner: shared_inner.clone() };
        let backend_b = MockBackend { inner: shared_inner.clone() };

        let mut sup_a = JackSupervisor::new(backend_a);
        let config = JackConfig::test_default();
        sup_a.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert!(sup_a.state(&name()).unwrap().is_ready());

        // Drop A WITHOUT shutting down — the shared backend keeps `running`
        // populated, simulating a jackd that survived the controller drop.
        drop(sup_a);
        assert!(shared_inner.lock().unwrap().running.contains(&name()));

        // Clear the recorded calls so we can assert cleanly against B alone.
        shared_inner.lock().unwrap().calls.clear();

        let mut sup_b = JackSupervisor::new(backend_b);
        let meta = sup_b.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.sample_rate, config.sample_rate);

        let calls = sup_b.backend.calls();
        assert!(
            !calls.iter().any(|c| matches!(c, MockCall::Spawn(_, _))),
            "supervisor B must adopt — not spawn"
        );
        assert!(
            calls.iter().any(|c| matches!(c, MockCall::ProbeMeta(_))),
            "supervisor B must probe during adoption"
        );
    }

    // Adoption when the socket is present but the server is unresponsive
    // (zombie) must terminate+respawn, never leave the supervisor stuck.
    #[test]
    fn ensure_server_adopts_zombie_by_terminating_and_respawning() {
        let mut sup = make_supervisor();
        sup.backend.inner.lock().unwrap().running.insert(name());
        // Script the first probe (the adoption probe) to fail.
        sup.backend.queue_probe_result(&name(), Err("zombie unresponsive".into()));

        let meta = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .expect("supervisor must recover from a zombie adoption");
        assert_eq!(meta.sample_rate, 48_000);

        let calls = sup.backend.calls();
        assert!(calls.iter().any(|c| matches!(c, MockCall::Terminate(_))));
        assert!(calls.iter().any(|c| matches!(c, MockCall::Spawn(_, _))));
    }

    // When desired config changes and clients are registered, the pre-kill
    // teardown hook must fire BEFORE backend.terminate. Invariant #1.
    #[test]
    fn ensure_server_runs_teardown_hook_before_terminate_when_config_changes() {
        let mut sup = make_supervisor();
        let config1 = JackConfig::test_default();
        sup.ensure_server(&name(), &config1, &mut noop_hook()).unwrap();
        sup.register_client(&name());
        sup.register_client(&name());

        let call_count_before_hook = std::sync::Arc::new(std::sync::Mutex::new(0usize));
        let observed_call_count = std::sync::Arc::clone(&call_count_before_hook);
        let calls_arc = sup.backend.inner.clone();
        let mut hook = {
            let observed_call_count = std::sync::Arc::clone(&observed_call_count);
            let calls_arc = calls_arc.clone();
            move |_: &ServerName| {
                let count = calls_arc.lock().unwrap().calls.len();
                *observed_call_count.lock().unwrap() = count;
            }
        };

        let config2 = JackConfig {
            buffer_size: 256,
            ..config1
        };
        sup.ensure_server(&name(), &config2, &mut hook).unwrap();

        let calls = sup.backend.calls();
        let hook_saw = *call_count_before_hook.lock().unwrap();
        let terminate_idx = calls
            .iter()
            .position(|c| matches!(c, MockCall::Terminate(_)))
            .expect("terminate must have been called");
        assert!(
            hook_saw <= terminate_idx,
            "teardown hook ran at call {} but terminate was at {}",
            hook_saw,
            terminate_idx
        );
        assert_eq!(sup.client_count(&name()), 0, "clients cleared post-teardown");
    }

    // With zero registered clients, the teardown hook is skipped. Terminate
    // still runs — the restart itself is independent of the hook.
    #[test]
    fn ensure_server_skips_teardown_hook_when_no_clients_registered() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let hook_fired = std::sync::Arc::new(std::sync::Mutex::new(false));
        let hook_flag = std::sync::Arc::clone(&hook_fired);
        let mut hook = move |_: &ServerName| {
            *hook_flag.lock().unwrap() = true;
        };
        let config2 = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        sup.ensure_server(&name(), &config2, &mut hook).unwrap();
        assert!(!*hook_fired.lock().unwrap(), "hook must not fire with zero clients");
    }

    // Restart event carries a ConfigMismatch reason with both old and new
    // configs. This is the signal the UI uses to explain the transient gap.
    #[test]
    fn ensure_server_emits_restart_requested_with_config_mismatch_reason() {
        let mut sup = make_supervisor();
        let rx = sup.events();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let config2 = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        sup.ensure_server(&name(), &config2, &mut noop_hook()).unwrap();

        let events: Vec<_> = rx.try_iter().collect();
        let restart_event = events
            .iter()
            .find(|e| matches!(e, SupervisorEvent::RestartRequested { .. }))
            .expect("RestartRequested must be emitted");
        match restart_event {
            SupervisorEvent::RestartRequested {
                reason: RestartReason::ConfigMismatch { old, new },
                ..
            } => {
                assert_eq!(old.buffer_size, 128);
                assert_eq!(new.buffer_size, 256);
            }
            other => panic!("unexpected reason: {:?}", other),
        }
    }

    // When post-ready reports SocketVanished on the first attempt but Healthy
    // on the second, the supervisor must emit BufferClampedTo and end up
    // Ready at the bumped buffer size.
    #[test]
    fn spawn_bumps_buffer_on_post_ready_socket_vanished() {
        let mut sup = make_supervisor();
        let rx = sup.events();
        sup.backend
            .queue_post_ready(&name(), PostReadyStatus::SocketVanished);
        // Second attempt succeeds.
        let config = JackConfig {
            buffer_size: 64,
            ..JackConfig::test_default()
        };
        let meta = sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.buffer_size, 128, "bumped to 2x");

        let events: Vec<_> = rx.try_iter().collect();
        let clamp_event = events
            .iter()
            .find(|e| matches!(e, SupervisorEvent::BufferClampedTo { .. }));
        assert!(clamp_event.is_some(), "BufferClampedTo must be emitted");
    }

    // DriverFailure is treated identically to SocketVanished from the
    // buffer-fallback perspective.
    #[test]
    fn spawn_bumps_buffer_on_post_ready_driver_failure() {
        let mut sup = make_supervisor();
        sup.backend
            .queue_post_ready(&name(), PostReadyStatus::DriverFailure("Broken pipe".into()));
        let config = JackConfig {
            buffer_size: 64,
            ..JackConfig::test_default()
        };
        // Speed up the test — we don't actually need the 2s sleep between
        // attempts here because the mock doesn't block on sockets.
        // (We accept the real delay; 2s is acceptable for one test.)
        let meta = sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert_eq!(meta.buffer_size, 128);
    }

    // All three attempts fail → state is Failed and ensure_server returns
    // Err. A subsequent ensure_server must be able to recover (no stuck
    // state).
    #[test]
    fn spawn_exhausts_attempts_and_transitions_to_failed() {
        let mut sup = make_supervisor();
        for _ in 0..MAX_SPAWN_ATTEMPTS {
            sup.backend.queue_spawn_result(Err("persistent".into()));
        }
        let err = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap_err();
        assert!(err.to_string().contains("persistent"));
        matches!(
            sup.state(&name()),
            Some(JackServerState::Failed { attempts, .. }) if *attempts == MAX_SPAWN_ATTEMPTS
        );

        // Next call should not be stuck — it performs another spawn attempt.
        let meta = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert_eq!(meta.sample_rate, 48_000);
    }

    // Health check on a Ready server whose socket has vanished (simulated
    // by the mock backend clearing `running`) transitions the verdict to
    // NotRunning. The supervisor intentionally avoids opening a libjack
    // client on every tick — the next ensure_server retry is what diagnoses
    // a zombie, not the health check.
    #[test]
    fn health_check_reports_not_running_when_socket_vanishes() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        // Simulate the jackd socket disappearing (e.g. USB disconnect).
        sup.backend.inner.lock().unwrap().running.remove(&name());

        let verdicts = sup.health_check();
        assert_eq!(verdicts.get(&name()), Some(&HealthStatus::NotRunning));
    }

    // Health check is pure filesystem introspection — it must not open any
    // libjack client. The old implementation did a `probe_meta` per tick,
    // which destabilised USB audio stacks on RK3588 and was the proximate
    // cause of the test-3 audio-stop regression during issue #308 hardware
    // validation.
    #[test]
    fn health_check_does_not_call_probe_meta() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let probes_before = sup
            .backend
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProbeMeta(_)))
            .count();
        let _ = sup.health_check();
        let _ = sup.health_check();
        let _ = sup.health_check();
        let probes_after = sup
            .backend
            .calls()
            .iter()
            .filter(|c| matches!(c, MockCall::ProbeMeta(_)))
            .count();
        assert_eq!(probes_before, probes_after, "health_check must not probe_meta");
    }

    // Health check on a server that was never started returns NotRunning.
    #[test]
    fn health_check_reports_not_running_for_unknown_server() {
        let mut sup = make_supervisor();
        let verdicts = sup.health_check();
        assert!(verdicts.is_empty(), "no servers means empty verdict map");
    }

    // stop_server drives Ready → NotStarted via terminate + forget and emits
    // the ServerStopped event. Counters are reset.
    #[test]
    fn stop_server_resets_state_and_emits_stopped_event() {
        let mut sup = make_supervisor();
        let rx = sup.events();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.register_client(&name());
        sup.stop_server(&name()).unwrap();

        assert!(matches!(sup.state(&name()), Some(JackServerState::NotStarted)));
        assert_eq!(sup.client_count(&name()), 0);
        let events: Vec<_> = rx.try_iter().collect();
        assert!(events.iter().any(|e| matches!(e, SupervisorEvent::ServerStopped { .. })));
    }

    // shutdown_all is idempotent — calling twice after a stop does nothing
    // but still returns Ok(()).
    #[test]
    fn shutdown_all_is_idempotent() {
        let mut sup = make_supervisor();
        sup.ensure_server(&"a".into(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.ensure_server(&"b".into(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.shutdown_all().unwrap();
        let first_round = sup.backend.call_count();
        sup.shutdown_all().unwrap();
        assert_eq!(
            sup.backend.call_count(),
            first_round,
            "second shutdown_all must not call the backend"
        );
    }

    // Registering a client then unregistering it bookkeeps the count; the
    // teardown hook is not fired when the count reaches zero before the
    // next restart.
    #[test]
    fn client_registration_counter_is_balanced() {
        let mut sup = make_supervisor();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert_eq!(sup.client_count(&name()), 0);
        sup.register_client(&name());
        sup.register_client(&name());
        assert_eq!(sup.client_count(&name()), 2);
        sup.unregister_client(&name());
        assert_eq!(sup.client_count(&name()), 1);
        sup.unregister_client(&name());
        assert_eq!(sup.client_count(&name()), 0);
        sup.unregister_client(&name()); // saturating
        assert_eq!(sup.client_count(&name()), 0);
    }

    // would_restart is side-effect-free and only returns true for
    // Ready(config mismatch) — the case where callers must drop AsyncClients.
    #[test]
    fn would_restart_distinguishes_mismatch_from_unseen_or_terminal_states() {
        let mut sup = make_supervisor();
        let config = JackConfig::test_default();

        // Unknown server — no restart needed, ensure_server will spawn.
        assert!(!sup.would_restart(&name(), &config));

        // After start, matching config → no restart.
        sup.ensure_server(&name(), &config, &mut noop_hook()).unwrap();
        assert!(!sup.would_restart(&name(), &config));

        // Mismatched config → restart required.
        let different = JackConfig {
            buffer_size: 256,
            ..config.clone()
        };
        assert!(sup.would_restart(&name(), &different));

        // Still no backend calls — would_restart is pure.
        let calls_before = sup.backend.call_count();
        let _ = sup.would_restart(&name(), &different);
        let _ = sup.would_restart(&name(), &different);
        assert_eq!(sup.backend.call_count(), calls_before);
    }

    // Basic sanity: each subscriber gets its own event stream.
    #[test]
    fn events_fan_out_to_multiple_subscribers() {
        let mut sup = make_supervisor();
        let rx1 = sup.events();
        let rx2 = sup.events();
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        let r1: Vec<_> = rx1.try_iter().collect();
        let r2: Vec<_> = rx2.try_iter().collect();
        assert!(!r1.is_empty());
        assert_eq!(r1.len(), r2.len());
    }

    // The meta() accessor only returns data for Ready servers; all other
    // states return Err.
    #[test]
    fn meta_accessor_requires_ready_state() {
        let mut sup = make_supervisor();
        assert!(sup.meta(&name()).is_err(), "unknown server");
        sup.ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert!(sup.meta(&name()).is_ok());
        sup.stop_server(&name()).unwrap();
        assert!(sup.meta(&name()).is_err(), "not-started after stop");
    }

    // The supervisor tolerates multiple concurrent server identities without
    // cross-contamination.
    #[test]
    fn multiple_servers_do_not_share_state() {
        let mut sup = make_supervisor();
        sup.ensure_server(&"a".into(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        sup.ensure_server(
            &"b".into(),
            &JackConfig {
                buffer_size: 256,
                ..JackConfig::test_default()
            },
            &mut noop_hook(),
        )
        .unwrap();
        let a_state = sup.state(&"a".into()).unwrap();
        let b_state = sup.state(&"b".into()).unwrap();
        match (a_state, b_state) {
            (
                JackServerState::Ready {
                    launched_config: ca, ..
                },
                JackServerState::Ready {
                    launched_config: cb, ..
                },
            ) => {
                assert_eq!(ca.buffer_size, 128);
                assert_eq!(cb.buffer_size, 256);
            }
            _ => panic!("both servers must be Ready"),
        }
    }

    // Mock meta override — used to make assertions about what probe returns.
    fn custom_meta() -> JackMeta {
        JackMeta {
            sample_rate: 44_100,
            buffer_size: 512,
            capture_port_count: 4,
            playback_port_count: 2,
            hw_name: "custom".into(),
        }
    }

    #[test]
    fn probe_meta_returned_by_ensure_server_is_the_backends_meta() {
        let mut sup = make_supervisor();
        sup.backend.queue_probe_result(&name(), Ok(custom_meta()));
        let meta = sup
            .ensure_server(&name(), &JackConfig::test_default(), &mut noop_hook())
            .unwrap();
        assert_eq!(meta, custom_meta());
    }
}
