//! Responsibility: lists the bindings of a chain whose device is absent.
//!
//! A chain whose interface is powered off or unplugged opens no stream,
//! so it must not switch on. The names returned are the bindings' display
//! names — what the user picked in the I/O screen — so the refusal can say
//! which interface is missing.

use domain::io_binding::IoBinding;
use project::binding_discovery::resolve_chain_ports;
use project::chain::Chain;

use crate::device_presence::DevicePresence;

/// Display names of the bindings `chain` uses whose device is not present,
/// once each, in port order.
pub fn chain_missing_devices(
    chain: &Chain,
    registry: &[IoBinding],
    presence: &dyn DevicePresence,
) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for port in resolve_chain_ports(chain, registry) {
        if presence.is_present(&port.endpoint.device_id, port.direction) {
            continue;
        }
        let name = registry
            .iter()
            .find(|b| b.id == port.binding_id)
            .map(|b| b.name.clone())
            .unwrap_or(port.binding_id);
        if !missing.contains(&name) {
            missing.push(name);
        }
    }
    missing
}
