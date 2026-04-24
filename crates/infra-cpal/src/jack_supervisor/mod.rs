//! `jack_supervisor` — single owner of every `jackd` process openrig
//! launches on Linux. Replaces the previous set of free functions
//! (`launch_jackd`, `ensure_jack_running`, `stop_jackd_for`, `jack_meta_for`)
//! and the global `JACKD_PIDS` / `JACK_META_CACHE` / `JACK_CONNECT_LOCK` /
//! `CARD_CHANNELS_REGISTRY` maps with an explicit state machine.
//!
//! See `supervisor.rs` for the documented invariants; see `backend.rs` for
//! the trait seam that lets us unit-test transitions without touching a real
//! JACK server.

pub mod backend;
pub mod supervisor;
pub mod types;

pub use backend::{JackBackend, MockBackend, PostReadyStatus};
pub use supervisor::JackSupervisor;
pub use types::{
    HealthStatus, JackConfig, JackMeta, JackServerState, RestartReason, ServerName,
    SupervisorEvent,
};
