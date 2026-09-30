//! Responsibility: formats the endpoint caption of a global-mixer strip.

use crate::mixer_strip::MixerDirection;

/// The strip's side and 1-based channels, as the interface prints them —
/// "IN 1", "OUT 1,2" — the same shape the chain meters use (#1006).
pub fn endpoint_channels_label(direction: MixerDirection, channels: &[usize]) -> String {
    let side = match direction {
        MixerDirection::Input => "IN",
        MixerDirection::Output => "OUT",
    };
    let list: Vec<String> = channels.iter().map(|c| (c + 1).to_string()).collect();
    format!("{side} {}", list.join(","))
}
