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

/// Maximum number of spawn attempts per `ensure_server` call. Kept low —
/// libjack state corruption (the "Cannot open shm segment" regression from
/// issue #294) is not recoverable within the same process lifetime, so
/// burning 10+ seconds of retry cycles before bailing just freezes the UI
/// for no gain. If the first attempt fails the user sees Failed quickly and
/// can restart the app.
const MAX_SPAWN_ATTEMPTS: u32 = 2;

/// Wall-clock delay between spawn retries. Kept short so the UI doesn't
/// hang — 500 ms is enough for ALSA to release the PCM after a failed
/// jackd exit, and much beyond that the user starts perceiving the app
/// as frozen.
const SPAWN_RETRY_DELAY: Duration = Duration::from_millis(500);

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
                        log::info!(
                            "supervisor::ensure_server: adopted '{}' (sr={} buf={} in={} out={}) — no spawn needed",
                            name,
                            meta.sample_rate,
                            meta.buffer_size,
                            meta.capture_port_count,
                            meta.playback_port_count
                        );
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
                    // Propagate termination failures up: ignoring them and
                    // falling through to spawn guarantees the safety check
                    // inside spawn will refuse, burying the real diagnostic
                    // ("a jackd we can't control is holding the device") in
                    // a "spawn refused" error.
                    self.emit(SupervisorEvent::RestartRequested {
                        name: name.clone(),
                        reason,
                    });
                    if let Err(e) = self.backend.terminate(name) {
                        self.backend.forget(name);
                        self.set_state(
                            name,
                            JackServerState::Failed {
                                last_error: e.to_string(),
                                attempts: 0,
                            },
                        );
                        self.emit(SupervisorEvent::ServerFailed {
                            name: name.clone(),
                            error: e.to_string(),
                        });
                        return Err(e);
                    }
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
                    if let Err(e) = self.backend.terminate(name) {
                        self.backend.forget(name);
                        self.set_state(
                            name,
                            JackServerState::Failed {
                                last_error: e.to_string(),
                                attempts: 0,
                            },
                        );
                        self.emit(SupervisorEvent::ServerFailed {
                            name: name.clone(),
                            error: e.to_string(),
                        });
                        return Err(e);
                    }
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
                    let previous = attempt_config.buffer_size;
                    attempt_config = bump_buffer(&attempt_config);
                    if attempt_config.buffer_size != previous {
                        self.emit(SupervisorEvent::BufferClampedTo {
                            name: name.clone(),
                            from: previous,
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
                    let previous = attempt_config.buffer_size;
                    attempt_config = bump_buffer(&attempt_config);
                    if attempt_config.buffer_size != previous {
                        self.emit(SupervisorEvent::BufferClampedTo {
                            name: name.clone(),
                            from: previous,
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
        if let Err(e) = self.backend.terminate(name) {
            // Leaving state as Restarting on failure was confusing — the
            // fast-path `if state is Ready` check would miss, spawn_with_
            // retries would run, and the user would get a "spawn refused:
            // socket present" burying the real cause. Transition to Failed
            // with the terminate error so the caller sees the truth.
            self.backend.forget(name);
            self.set_state(
                name,
                JackServerState::Failed {
                    last_error: e.to_string(),
                    attempts: 0,
                },
            );
            self.emit(SupervisorEvent::ServerFailed {
                name: name.clone(),
                error: e.to_string(),
            });
            return Err(e);
        }
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

    /// Check whether `desired` differs from the current `Ready` state on a
    /// single axis: `buffer_size`. Used by `ensure_jack_servers` to route
    /// buffer-only deltas through `jack_set_buffer_size` on a live client
    /// instead of the terminate+spawn path (which risks libjack state
    /// corruption on some Linux deployments — issue #294 / #308 bug 1).
    ///
    /// Returns false when anything else differs (sample_rate, card, channel
    /// counts, nperiods, realtime flags), when the server is not `Ready`, or
    /// when the buffer size already matches.
    pub fn only_buffer_changed(&self, name: &ServerName, desired: &JackConfig) -> bool {
        let Some(JackServerState::Ready { launched_config, .. }) =
            self.servers.get(name).map(|s| &s.state)
        else {
            return false;
        };
        launched_config.buffer_size != desired.buffer_size
            && launched_config.sample_rate == desired.sample_rate
            && launched_config.card_num == desired.card_num
            && launched_config.capture_channels == desired.capture_channels
            && launched_config.playback_channels == desired.playback_channels
            && launched_config.nperiods == desired.nperiods
            && launched_config.realtime == desired.realtime
            && launched_config.rt_priority == desired.rt_priority
    }

    /// Update the supervisor's cached launched_config + meta to reflect a
    /// successful in-place buffer resize. Call this AFTER a client's
    /// `set_buffer_size` succeeded, so `would_restart` stops reporting a
    /// mismatch on the next `ensure_server` tick.
    pub fn mark_buffer_resized(&mut self, name: &ServerName, new_buffer: u32) {
        if let Some(server) = self.servers.get_mut(name) {
            if let JackServerState::Ready {
                meta,
                launched_config,
                ..
            } = &mut server.state
            {
                meta.buffer_size = new_buffer;
                launched_config.buffer_size = new_buffer;
                log::info!(
                    "supervisor: '{}' launched_config buffer_size → {} (live resize)",
                    name,
                    new_buffer
                );
            }
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
#[path = "supervisor_tests.rs"]
mod tests;
