//! Responsibility: classifies a desired config against the server already running.
//!
//! A buffer-only delta is applied live through `jack_set_buffer_size` (no
//! jackd restart, no libjack state corruption — the #294 regression); every
//! other delta means the server has to come down.

use super::backend::JackBackend;
use super::supervisor::JackSupervisor;
use super::types::{JackConfig, JackServerState, ServerName};

impl<B: JackBackend> JackSupervisor<B> {
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
        // Restart ONLY for params jackd cannot change on a live server:
        // ALSA device (`card_num`), sample rate (no live SR API), and
        // buffer size (the live-resize fast-path runs *before* this is
        // consulted; a residual delta here means soft-resize failed and
        // a restart is the fallback). Channel counts / nperiods /
        // rt_priority / realtime are EXCLUDED — they differ run-to-run
        // from device re-detection (a 4-capture-channel Scarlett,
        // default vs stored values) with NO port-shape change. Comparing
        // the whole struct made every chain edit (even a pure block
        // swap) spuriously report "restart", driving the destructive
        // stop()+kill+libjack-recreate that wedges into the #294/#308
        // "Cannot open shm segment" limbo and freezes the app. Mirrors
        // `ensure_server`'s adopt criteria so the predicates agree (#479).
        matches!(
            self.servers.get(name).map(|s| &s.state),
            Some(JackServerState::Ready { launched_config, .. })
                if launched_config.sample_rate != desired.sample_rate
                    || launched_config.buffer_size != desired.buffer_size
                    || launched_config.card_num != desired.card_num
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
        let Some(JackServerState::Ready {
            launched_config, ..
        }) = self.servers.get(name).map(|s| &s.state)
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
}
