//! Responsibility: retries a jackd spawn until the driver accepts the buffer size.
//!
//! A `buf=64` the driver rejects with "Broken pipe" is bumped (128, 256, …)
//! up to `MAX_BUFFER_CLAMP` before the server is declared failed.

use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};

use super::backend::{JackBackend, PostReadyStatus};
use super::supervisor::JackSupervisor;
use super::types::{
    HealthStatus, JackConfig, JackMeta, JackServerState, ServerName, SupervisorEvent,
};

/// Maximum number of spawn attempts per `ensure_server` call. Kept low —
/// libjack state corruption (the "Cannot open shm segment" regression from
/// issue #294) is not recoverable within the same process lifetime, so
/// burning 10+ seconds of retry cycles before bailing just freezes the UI
/// for no gain. If the first attempt fails the user sees Failed quickly and
/// can restart the app.
pub(super) const MAX_SPAWN_ATTEMPTS: u32 = 2;

/// Wall-clock delay between spawn retries. Kept short so the UI doesn't
/// hang — 500 ms is enough for ALSA to release the PCM after a failed
/// jackd exit, and much beyond that the user starts perceiving the app
/// as frozen.
pub(super) const SPAWN_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Upper bound on the buffer-size fallback growth. `buf=64` that trips
/// "Broken pipe" gets bumped to 128, then 256, then 512, then 1024 — beyond
/// that we declare defeat and fail.
pub(super) const MAX_BUFFER_CLAMP: u32 = 1024;

impl<B: JackBackend> JackSupervisor<B> {
    /// Attempt up to [`MAX_SPAWN_ATTEMPTS`] spawns with exponential buffer
    /// fallback on post-ready driver failure. Moves the server to `Ready` on
    /// success, `Failed` on exhaustion.
    pub(super) fn spawn_with_retries(
        &mut self,
        name: &ServerName,
        desired: &JackConfig,
    ) -> Result<JackMeta> {
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
}

fn bump_buffer(config: &JackConfig) -> JackConfig {
    let bumped = (config.buffer_size * 2).min(MAX_BUFFER_CLAMP);
    JackConfig {
        buffer_size: bumped,
        ..config.clone()
    }
}
