//! Responsibility: serializes the global mixer strips for a read.
//! #1007: one entry per configured endpoint, with the fader, mute and solo the
//! dispatcher holds — the read half of the mixer commands.

use crate::mixer_view::MixerStripView;

/// `{"strips": [...]}` in the order the view lists them (inputs first).
pub fn mixer_state_json(strips: &[MixerStripView]) -> String {
    let strips: Vec<serde_json::Value> = strips
        .iter()
        .map(|s| {
            serde_json::json!({
                "id": s.id,
                "direction": s.direction,
                "name": s.name,
                "device_id": s.device_id,
                "channels": s.channels,
                "gain_db": s.gain_db,
                "muted": s.muted,
                "soloed": s.soloed,
            })
        })
        .collect();
    serde_json::json!({ "strips": strips }).to_string()
}
