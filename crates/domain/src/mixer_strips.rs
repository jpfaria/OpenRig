//! Responsibility: derives the global-mixer strips from the configured endpoint bindings.

use crate::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use crate::mixer_strip::{MixerDirection, MixerStripId};

/// One fader of the global mixer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixerStrip {
    pub id: MixerStripId,
    /// Name of the first endpoint that declares this physical endpoint.
    pub name: String,
    pub mode: ChannelMode,
}

/// Every distinct physical endpoint in `bindings`, inputs first, in
/// declaration order. The same device + channels declared in several
/// bindings is one strip.
pub fn strips_from_bindings(bindings: &[IoBinding]) -> Vec<MixerStrip> {
    let inputs = bindings
        .iter()
        .flat_map(|b| b.inputs.iter().map(|e| (MixerDirection::Input, e)));
    let outputs = bindings
        .iter()
        .flat_map(|b| b.outputs.iter().map(|e| (MixerDirection::Output, e)));
    let mut strips: Vec<MixerStrip> = Vec::new();
    for (direction, endpoint) in inputs.chain(outputs) {
        let id = strip_id(direction, endpoint);
        if !strips.iter().any(|s| s.id == id) {
            strips.push(MixerStrip {
                id,
                name: endpoint.name.clone(),
                mode: endpoint.mode,
            });
        }
    }
    strips
}

fn strip_id(direction: MixerDirection, endpoint: &IoEndpoint) -> MixerStripId {
    MixerStripId {
        direction,
        device_id: endpoint.device_id.0.clone(),
        channels: endpoint.channels.clone(),
    }
}
