//! #829 — re-enumerating the audio devices as a command.
//!
//! A USB hot-swap was only recoverable by clicking the GUI's refresh
//! button, so MCP / gRPC could not drive it. The command mutates no project
//! state: the dispatcher emits the event and each frontend re-reads the
//! device list (and re-checks bindings that reference an absent device).

use anyhow::Result;

use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;

impl LocalDispatcher {
    pub(crate) fn handle_refresh_audio_devices(&self) -> Result<Vec<Event>> {
        Ok(vec![Event::AudioDevicesRefreshed])
    }
}
