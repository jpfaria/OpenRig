//! Responsibility: answers the dispatcher's device-presence question from the audio host.
//!
//! The desktop app owns the audio host, so it is the one that can say
//! whether a chain's interface is there before the chain switches on. MCP is
//! served by this same dispatcher, so it gets the same refusal.

use std::rc::Rc;

use application::device_presence::DevicePresence;
use domain::ids::DeviceId;
use project::binding_discovery::PortDirection;

use crate::state::ProjectSession;

struct HostDevicePresence;

impl DevicePresence for HostDevicePresence {
    fn is_present(&self, device: &DeviceId, direction: PortDirection) -> bool {
        infra_cpal::device_is_present(device, direction)
    }
}

/// Give the session's dispatcher the host's presence check. Idempotent.
pub(crate) fn attach_device_presence(session: &ProjectSession) {
    session
        .dispatcher
        .attach_device_presence(Rc::new(HostDevicePresence));
}
