//! Responsibility: declares the port that answers whether an audio device is present.
//!
//! The dispatcher refuses to switch on a chain whose interface is not
//! there, but it hosts no audio backend — the frontend that owns one (today
//! `adapter-gui`) attaches this port. A dispatcher with none attached (MCP-only,
//! tests) asks nothing and enables as before.

use domain::ids::DeviceId;
use project::binding_discovery::PortDirection;

/// Whether the host lists `device` for the given direction right now.
pub trait DevicePresence {
    fn is_present(&self, device: &DeviceId, direction: PortDirection) -> bool;
}
