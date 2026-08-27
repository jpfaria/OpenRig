//! Responsibility: turns a device's channels into what the picker shows.

use crate::ChannelOptionItem;
use domain::io_binding::ChannelMode;
use domain::AudioDeviceDescriptor;

/// Parse the snake_case wire token into a `ChannelMode`. Unknown tokens fall
/// back to `Mono` (the domain default); the picker only ever emits the three
/// valid tokens, so this is a defensive default, not a hardcoded device value.
pub(crate) fn channel_mode_from_str(s: &str) -> ChannelMode {
    match s {
        "stereo" => ChannelMode::Stereo,
        "dual_mono" => ChannelMode::DualMono,
        _ => ChannelMode::Mono,
    }
}

/// Build the per-channel checkbox options for `device_id`, derived from the
/// device's reported channel count. `selected` marks which 0-based indices are
/// currently chosen. An unknown device id yields an empty list (no fallback to
/// a default device or a hardcoded channel count).
pub(crate) fn channel_items_for_device(
    device_id: &str,
    devices: &[AudioDeviceDescriptor],
    selected: &[usize],
) -> Vec<ChannelOptionItem> {
    let Some(device) = devices.iter().find(|d| d.id == device_id) else {
        return Vec::new();
    };
    (0..device.channels)
        .map(|channel| ChannelOptionItem {
            index: channel as i32,
            label: rust_i18n::t!("label-channel-numbered", n = channel + 1)
                .to_string()
                .into(),
            selected: selected.contains(&channel),
            available: true,
        })
        .collect()
}

/// Apply a channel toggle honouring the mode's selection rule. Mono is a radio
/// group (exactly one channel): selecting a channel deselects every other.
/// Stereo and DualMono are checkbox sets where multiple channels may be
/// selected at once. Returns the updated option list (pure — no model mutation).
pub(crate) fn apply_channel_toggle(
    items: &[ChannelOptionItem],
    index: i32,
    selected: bool,
    mode: ChannelMode,
) -> Vec<ChannelOptionItem> {
    let single_select = mode == ChannelMode::Mono;
    items
        .iter()
        .map(|item| {
            let mut next = item.clone();
            if item.index == index {
                next.selected = selected;
            } else if single_select && selected {
                // Radio behaviour: choosing one clears the rest.
                next.selected = false;
            }
            next
        })
        .collect()
}

/// Snake_case wire token for a `ChannelMode`, for the read-back display models.
pub(crate) fn mode_label(mode: ChannelMode) -> &'static str {
    match mode {
        ChannelMode::Mono => "mono",
        ChannelMode::Stereo => "stereo",
        ChannelMode::DualMono => "dual_mono",
    }
}

/// Sequential default endpoint name ("In N" / "Out N") so a structured add
/// always yields a labelled, removable endpoint without free text.
pub(crate) fn next_endpoint_name(existing: usize, is_input: bool) -> String {
    let prefix = if is_input { "In" } else { "Out" };
    format!("{prefix} {}", existing + 1)
}
