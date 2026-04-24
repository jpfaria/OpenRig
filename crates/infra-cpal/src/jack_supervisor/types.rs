//! Core data types for the JACK supervisor: server identity, desired config,
//! observed metadata, server state machine, restart causes, health codes and
//! the observable event stream.
//!
//! These types are platform-agnostic on purpose — the state machine they
//! describe is the one we test against, and the live JACK backend is simply
//! one of its implementations. The supervisor code in `supervisor.rs` owns all
//! the control logic; the types here only describe facts.

use std::fmt;
use std::time::Instant;

/// A JACK server name — the string passed to `jackd -n <name>`. Named servers
/// let openrig run one jackd per USB audio interface without clobbering each
/// other's sockets in `/dev/shm`.
///
/// Wrapped as a newtype so we never confuse a server name with a device id,
/// client name or chain id at call sites.
#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct ServerName(String);

impl ServerName {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<&str> for ServerName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for ServerName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Desired configuration for a JACK server. Fully describes how `jackd` should
/// be launched. `JackSupervisor::ensure_server` compares the currently
/// launched `JackConfig` (if any) to the desired one and triggers a restart
/// when they differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JackConfig {
    pub sample_rate: u32,
    pub buffer_size: u32,
    /// ALSA `-n` periods per buffer. Typical values 2 or 3.
    pub nperiods: u32,
    /// Whether jackd should be launched with `--realtime` + `-P <rt_priority>`.
    pub realtime: bool,
    /// SCHED_FIFO priority when `realtime` is true. Ignored otherwise.
    pub rt_priority: u8,
    /// ALSA card number (for `-d hw:<card_num>`).
    pub card_num: u32,
    /// Capture channel count (for `-i`).
    pub capture_channels: u32,
    /// Playback channel count (for `-o`).
    pub playback_channels: u32,
}

impl JackConfig {
    /// Minimal config used in tests. Real call sites should not use this.
    #[cfg(test)]
    pub fn test_default() -> Self {
        Self {
            sample_rate: 48_000,
            buffer_size: 128,
            nperiods: 3,
            realtime: false,
            rt_priority: 70,
            card_num: 1,
            capture_channels: 2,
            playback_channels: 2,
        }
    }
}

/// Metadata exposed by a running JACK server — all the numbers openrig needs
/// to resolve chain audio configs (sample_rate, buffer_size, port counts).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JackMeta {
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub capture_port_count: usize,
    pub playback_port_count: usize,
    pub hw_name: String,
}

/// Explicit state machine for a single JACK server. Transitions are driven by
/// `JackSupervisor::ensure_server`, `stop_server` and `health_check`. A server
/// starts in `NotStarted` and only enters `Ready` after its post-ready probe
/// confirms it did not die immediately after opening its socket.
#[derive(Clone, Debug)]
pub enum JackServerState {
    NotStarted,
    Spawning {
        started_at: Instant,
        desired: JackConfig,
    },
    Ready {
        meta: JackMeta,
        launched_config: JackConfig,
        ready_at: Instant,
    },
    Restarting {
        reason: RestartReason,
    },
    Failed {
        last_error: String,
        attempts: u32,
    },
}

impl JackServerState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::NotStarted | Self::Failed { .. })
    }

    /// Returns the currently-launched config when the server is `Ready`,
    /// otherwise `None`. Used by `ensure_server` to decide whether the desired
    /// config matches what is actually running.
    pub fn launched_config(&self) -> Option<&JackConfig> {
        match self {
            Self::Ready { launched_config, .. } => Some(launched_config),
            _ => None,
        }
    }
}

/// Why a server transitioned from `Ready` back into `Restarting`. Emitted as a
/// `SupervisorEvent::RestartRequested` so the UI layer can explain the gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestartReason {
    /// The user (or another change in project state) picked a different
    /// sample rate or buffer size.
    ConfigMismatch {
        old: JackConfig,
        new: JackConfig,
    },
    /// Socket exists but the server isn't responding to client connections.
    /// Seen after USB disconnects on RK3588: jackd's process survives but the
    /// ALSA driver has gone away.
    Zombie,
    /// The supervisor's periodic health check marked the server unhealthy.
    HealthCheckFailed { detail: String },
    /// Requested explicitly (e.g. from a "Restart JACK" UI affordance).
    UserRequested,
    /// The post-ready probe detected that jackd exited right after startup
    /// (typically ALSA "Broken pipe" at a too-small buffer). The supervisor
    /// will retry the spawn, potentially with a larger buffer.
    BufferTooSmall { failed: u32 },
}

/// Observable event emitted on every state transition. Consumers (UI, logging)
/// subscribe via `JackSupervisor::events()` which hands out a new `mpsc`
/// receiver per caller.
#[derive(Clone, Debug)]
pub enum SupervisorEvent {
    ServerSpawning {
        name: ServerName,
        config: JackConfig,
    },
    ServerReady {
        name: ServerName,
        meta: JackMeta,
    },
    ServerFailed {
        name: ServerName,
        error: String,
    },
    ServerDied {
        name: ServerName,
    },
    ServerStopped {
        name: ServerName,
    },
    RestartRequested {
        name: ServerName,
        reason: RestartReason,
    },
    /// Emitted when the supervisor had to fall back to a larger buffer after
    /// the requested one caused a post-ready Broken-pipe failure. The UI
    /// should surface this so the user can see why the latency is higher than
    /// the setting they picked.
    BufferClampedTo {
        name: ServerName,
        from: u32,
        to: u32,
    },
    /// Fired when a pre-kill teardown hook was invoked because the supervisor
    /// was about to restart a server that still had clients registered.
    TeardownRequested {
        name: ServerName,
    },
}

/// Non-destructive health verdict, produced by `JackSupervisor::health_check`.
/// Callers don't act on it directly — the next `ensure_server` uses the
/// recorded verdict to decide whether a restart is needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Zombie,
    NotRunning,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_name_roundtrips_through_string_and_ref() {
        let a = ServerName::from("card1");
        let b = ServerName::new(String::from("card1"));
        let c: ServerName = String::from("card1").into();
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(a.as_str(), "card1");
        assert_eq!(format!("{}", a), "card1");
    }

    #[test]
    fn jack_server_state_ready_and_terminal_match_variant() {
        let meta = JackMeta {
            sample_rate: 48_000,
            buffer_size: 128,
            capture_port_count: 2,
            playback_port_count: 2,
            hw_name: "hw".into(),
        };
        let ready = JackServerState::Ready {
            meta,
            launched_config: JackConfig::test_default(),
            ready_at: Instant::now(),
        };
        assert!(ready.is_ready());
        assert!(!ready.is_terminal());
        assert!(ready.launched_config().is_some());

        let not_started = JackServerState::NotStarted;
        assert!(!not_started.is_ready());
        assert!(not_started.is_terminal());
        assert!(not_started.launched_config().is_none());

        let failed = JackServerState::Failed {
            last_error: "boom".into(),
            attempts: 3,
        };
        assert!(!failed.is_ready());
        assert!(failed.is_terminal());
    }

    #[test]
    fn restart_reason_config_mismatch_carries_both_configs() {
        let old = JackConfig::test_default();
        let new = JackConfig {
            buffer_size: 256,
            ..JackConfig::test_default()
        };
        let reason = RestartReason::ConfigMismatch {
            old: old.clone(),
            new: new.clone(),
        };
        match reason {
            RestartReason::ConfigMismatch { old: o, new: n } => {
                assert_eq!(o.buffer_size, 128);
                assert_eq!(n.buffer_size, 256);
            }
            other => panic!("unexpected variant: {:?}", other),
        }
    }
}
