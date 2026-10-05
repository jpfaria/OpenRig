//! Responsibility: picks the JACK server a set of device ids belongs to.
//!
//! A `jack:<name>` device id names it, a `hw:<N>` id is looked up among the
//! USB cards, otherwise the first card whose server is running, and finally
//! `default`.

#![cfg(all(target_os = "linux", feature = "jack"))]

use crate::drums_jack_ports::jack_server_for_device;
use crate::usb_proc::{jack_server_is_running_for, UsbAudioCard};

/// The JACK server name the first resolvable device id belongs to.
pub(crate) fn resolve_jack_server<'a>(
    cards: &[UsbAudioCard],
    device_ids: impl IntoIterator<Item = &'a str>,
) -> String {
    device_ids
        .into_iter()
        .find_map(|device_id| {
            jack_server_for_device(device_id, |hw_num| {
                cards
                    .iter()
                    .find(|c| c.card_num == hw_num)
                    .map(|c| c.server_name.clone())
            })
        })
        .or_else(|| {
            cards
                .iter()
                .find(|c| jack_server_is_running_for(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .unwrap_or_else(|| "default".to_string())
}
