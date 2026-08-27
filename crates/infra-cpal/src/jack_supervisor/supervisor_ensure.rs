//! Responsibility: brings one jackd server up in the configuration the caller asked for.
//!
//! Invariant 1 of the supervisor lives here: the pre-restart teardown hook
//! runs BEFORE `terminate` whenever the transition destroys a previously
//! `Ready` server, so callers can drop their `AsyncClient` handles before
//! the jackd they reference disappears.

use anyhow::Result;
use std::time::Instant;

use super::backend::JackBackend;
use super::supervisor::{JackServer, JackSupervisor};
use super::types::{
    HealthStatus, JackConfig, JackMeta, JackServerState, RestartReason, ServerName, SupervisorEvent,
};

impl<B: JackBackend> JackSupervisor<B> {
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
                    JackServerState::Ready {
                        launched_config, ..
                    } => RestartReason::ConfigMismatch {
                        old: launched_config.clone(),
                        new: desired.clone(),
                    },
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

    /// Invariant-preserving transition into `Restarting`. Fires the pre-kill
    /// teardown hook when any clients are registered, emits the restart
    /// event, and calls `backend.terminate` + `backend.forget` in that order.
    pub(super) fn transition_to_restarting(
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
}
