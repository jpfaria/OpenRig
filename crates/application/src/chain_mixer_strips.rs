//! Responsibility: lists the global-mixer strips a single chain plays through.
//! #1007: the compact view shows a chain's own inputs and outputs inline, so
//! it needs exactly the strips that chain plays through — never the rest of
//! the machine's endpoints.

use domain::io_binding::IoBinding;
use domain::mixer_strip::{MixerDirection, MixerStripId};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;

/// Wire ids of the strips `chain` uses, inputs first, each once.
pub fn chain_mixer_strip_ids(chain: &Chain, registry: &[IoBinding]) -> Vec<String> {
    let ports = resolve_chain_ports(chain, registry);
    let (inputs, outputs): (Vec<_>, Vec<_>) = ports
        .into_iter()
        .partition(|port| port.direction == PortDirection::Input);
    let mut ids: Vec<String> = Vec::new();
    for (direction, port) in inputs
        .into_iter()
        .map(|p| (MixerDirection::Input, p))
        .chain(outputs.into_iter().map(|p| (MixerDirection::Output, p)))
    {
        let id = MixerStripId {
            direction,
            device_id: port.endpoint.device_id.0,
            channels: port.endpoint.channels,
        }
        .to_wire();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}
