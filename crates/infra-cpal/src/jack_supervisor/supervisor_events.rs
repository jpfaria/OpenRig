//! Responsibility: publishes each supervisor event to its subscribers.
//!
//! Every transition is logged on the way out too: on hardware the journal is
//! the primary debugging channel for audio-stream incidents, and there is
//! usually no subscriber attached.

use super::backend::JackBackend;
use super::supervisor::JackSupervisor;
use super::types::SupervisorEvent;

impl<B: JackBackend> JackSupervisor<B> {
    pub(super) fn emit(&self, event: SupervisorEvent) {
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
