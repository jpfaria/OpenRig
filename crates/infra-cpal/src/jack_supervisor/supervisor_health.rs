//! Responsibility: records a health verdict for every server the supervisor tracks.
//!
//! Invariant 4: this is non-destructive — a bad verdict only gets recorded,
//! the actual restart happens on the next `ensure_server`.

use std::collections::HashMap;

use super::backend::JackBackend;
use super::supervisor::JackSupervisor;
use super::types::{HealthStatus, JackServerState, ServerName, SupervisorEvent};

impl<B: JackBackend> JackSupervisor<B> {
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
}
