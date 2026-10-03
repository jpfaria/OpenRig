//! Responsibility: opens a JACK client on a named server.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::{anyhow, Result};

use crate::jack_supervisor;

/// Open `client_name` on `server_name`, retrying up to 5 times 200 ms apart:
/// the JACK UNIX socket appears before the shm segments are fully
/// initialized, so the first attempt can fail with "Cannot open shm segment".
pub(crate) fn open_jack_client(server_name: &str, client_name: &str) -> Result<jack::Client> {
    let mut attempt = 0u32;
    loop {
        let result = {
            let _lock = jack_supervisor::live_backend::JACK_DEFAULT_SERVER_LOCK
                .lock()
                .unwrap();
            std::env::set_var("JACK_DEFAULT_SERVER", server_name);
            let r = jack::Client::new(client_name, jack::ClientOptions::NO_START_SERVER);
            std::env::remove_var("JACK_DEFAULT_SERVER");
            r
        };
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
