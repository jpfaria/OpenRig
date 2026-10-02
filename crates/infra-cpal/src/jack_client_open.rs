//! Responsibility: opens a JACK client against the server that owns a set of device ids.
//!
//! Server pick: a `jack:<name>` device id names it, a `hw:<N>` id is looked up
//! among the USB cards, otherwise the first card whose server is running, and
//! finally `default`. The open retries 5 times 200 ms apart to ride out the
//! libjack race where the server socket exists before its shm segments do.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::{anyhow, Result};

use crate::jack_supervisor;
use crate::usb_proc::{detect_all_usb_audio_cards, jack_server_is_running_for};

/// The JACK server name the first resolvable device id belongs to.
pub(crate) fn resolve_jack_server<'a>(device_ids: impl IntoIterator<Item = &'a str>) -> String {
    let cards = detect_all_usb_audio_cards();
    device_ids
        .into_iter()
        .find_map(|device_id| {
            if let Some(name) = device_id.strip_prefix("jack:") {
                return Some(name.to_string());
            }
            let hw_num = device_id.strip_prefix("hw:")?;
            cards
                .iter()
                .find(|c| c.card_num == hw_num)
                .map(|c| c.server_name.clone())
        })
        .or_else(|| {
            cards
                .iter()
                .find(|c| jack_server_is_running_for(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .unwrap_or_else(|| "default".to_string())
}

/// A new, inactive client named `client_name` on `server_name`.
pub(crate) fn open_jack_client(server_name: &str, client_name: &str) -> Result<jack::Client> {
    let mut attempt = 0u32;
    loop {
        let lock = jack_supervisor::live_backend::JACK_DEFAULT_SERVER_LOCK
            .lock()
            .unwrap();
        std::env::set_var("JACK_DEFAULT_SERVER", server_name);
        let result = jack::Client::new(client_name, jack::ClientOptions::NO_START_SERVER);
        std::env::remove_var("JACK_DEFAULT_SERVER");
        drop(lock);
        match result {
            Ok((client, _status)) => return Ok(client),
            Err(e) if attempt < 4 => {
                attempt += 1;
                log::warn!(
                    "JACK client '{}' connect attempt {} failed ({:?}), retrying in 200ms",
                    client_name,
                    attempt,
                    e
                );
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                return Err(anyhow!(
                    "failed to create JACK client for server '{}': {:?}",
                    server_name,
                    e
                ))
            }
        }
    }
}
