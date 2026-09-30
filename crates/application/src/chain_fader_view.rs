//! Responsibility: reads a chain's own fader on each strip it plays through.
//! #1007: the value lives in the project (`Chain.mix`), keyed by the chain's
//! port; the compact view addresses it by the global strip id. A strip the
//! chain reaches through several bindings reads the fader of its FIRST port,
//! the same port the chain mixer commands write.

use domain::io_binding::IoBinding;
use domain::mixer_strip::{MixerDirection, MixerStripId};
use project::binding_discovery::{resolve_chain_ports, PortDirection};
use project::chain::Chain;

/// A chain's own fader on one strip, as a frontend draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct ChainFaderView {
    /// Global strip wire id — what the chain mixer commands address.
    pub strip: String,
    pub gain_db: f32,
    pub muted: bool,
}

/// One view per strip `chain` plays through, inputs first, each once.
pub fn chain_fader_views(chain: &Chain, registry: &[IoBinding]) -> Vec<ChainFaderView> {
    let mut ports = resolve_chain_ports(chain, registry);
    // Stable: inputs first, each side in the order the chain resolves it.
    ports.sort_by_key(|port| port.direction != PortDirection::Input);
    let mut views: Vec<ChainFaderView> = Vec::new();
    for port in ports {
        let direction = match port.direction {
            PortDirection::Input => MixerDirection::Input,
            PortDirection::Output => MixerDirection::Output,
        };
        let strip = MixerStripId {
            direction,
            device_id: port.endpoint.device_id.0.clone(),
            channels: port.endpoint.channels.clone(),
        }
        .to_wire();
        if views.iter().any(|view| view.strip == strip) {
            continue;
        }
        let fader = chain
            .mix
            .endpoint(direction, &port.binding_id, &port.endpoint.name);
        views.push(ChainFaderView {
            strip,
            gain_db: fader.map_or(0.0, |f| f.gain_db),
            muted: fader.is_some_and(|f| f.muted),
        });
    }
    views
}
