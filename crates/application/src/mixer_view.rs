//! Responsibility: shapes the global mixer strips a frontend renders.
//! #1007: the strips come from the machine's I/O bindings; each carries the
//! setting the dispatcher holds for it.

use domain::io_binding::IoBinding;
use domain::mixer_strip::MixerDirection;
use domain::mixer_strips::strips_from_bindings;

use crate::mixer_state::MixerControlState;

/// One strip as a frontend or a transport reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct MixerStripView {
    /// Wire id — what the mixer commands address.
    pub id: String,
    pub direction: MixerDirection,
    /// The endpoint's name, from the first binding that declares it.
    pub name: String,
    pub device_id: String,
    pub channels: Vec<usize>,
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
}

/// Every strip the bindings expose, inputs first.
pub fn mixer_view(bindings: &[IoBinding], state: &MixerControlState) -> Vec<MixerStripView> {
    strips_from_bindings(bindings)
        .into_iter()
        .map(|strip| {
            let id = strip.id.to_wire();
            let setting = state.get(&id);
            MixerStripView {
                id,
                direction: strip.id.direction,
                name: strip.name,
                device_id: strip.id.device_id,
                channels: strip.id.channels,
                gain_db: setting.gain_db,
                muted: setting.muted,
                soloed: setting.soloed,
            }
        })
        .collect()
}
