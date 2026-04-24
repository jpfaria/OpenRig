//! `jack_supervisor` — single owner of every `jackd` process openrig
//! launches on Linux. Replaces the previous set of free functions
//! (`launch_jackd`, `ensure_jack_running`, `stop_jackd_for`, `jack_meta_for`)
//! and the global `JACKD_PIDS` / `JACK_META_CACHE` / `JACK_CONNECT_LOCK` /
//! `CARD_CHANNELS_REGISTRY` maps with an explicit state machine.
//!
//! See `supervisor.rs` for the documented invariants; see `backend.rs` for
//! the trait seam that lets us unit-test transitions without touching a real
//! JACK server.
//!
//! ## Drop semantics
//!
//! `JackSupervisor` deliberately does NOT terminate supervised `jackd`
//! processes on drop. Two code paths rely on that:
//!
//! 1. `start_jack_in_background` creates a one-shot supervisor for the
//!    launcher's "start audio" affordance, then drops it — the jackd it
//!    started is picked up by the later `ProjectRuntimeController` through
//!    the adoption path in `JackSupervisor::ensure_server`.
//!
//! 2. The GUI drops `ProjectRuntimeController` whenever all chains are
//!    disabled (see `sync_live_chain_runtime` in adapter-gui). Toggling a
//!    chain back on creates a fresh controller + supervisor, which adopts
//!    the still-running `jackd`. Killing `jackd` on drop would trigger a
//!    full respawn on every chain toggle — unnecessary work and a user-
//!    visible audio gap.
//!
//! Callers that need a final shutdown (app exit, test teardown) must invoke
//! `JackSupervisor::shutdown_all` explicitly.

pub mod backend;
pub mod supervisor;
pub mod types;

#[cfg(all(target_os = "linux", feature = "jack"))]
pub mod live_backend;

// Re-exports used by `crate::ProjectRuntimeController`. Internal types
// (`JackBackend`, `MockBackend`, `PostReadyStatus`, `RestartReason`,
// `SupervisorEvent`, `JackMeta`) stay reachable via their defining submodules
// so they only count as "used" when someone actually imports them — keeps the
// warning surface minimal on non-Linux builds.
pub use supervisor::JackSupervisor;
pub use types::{HealthStatus, JackConfig, JackServerState, ServerName};

#[cfg(all(target_os = "linux", feature = "jack"))]
pub use live_backend::LiveJackBackend;
