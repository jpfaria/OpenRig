//! Responsibility: reads a running JACK server's metadata over libjack
//!
//! Split out of `live_backend.rs` (#873). Kept separate from the backend
//! because device enumeration probes servers it does not supervise, so this
//! path must stay reachable without a `LiveJackBackend` instance.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::{anyhow, Result};
use std::sync::Mutex;
use std::time::Duration;

use super::types::{JackMeta, ServerName};

/// Number of client-open retries inside [`probe_server_meta`]. Kept low — the
/// transient shm-init race typically clears in 1-2 retries; beyond that a
/// persistent "Cannot open shm segment" signals libjack process-wide state
/// corruption (issue #294 / #308) which no amount of retrying recovers in
/// the same process lifetime. Capping at 3 keeps the UI freeze under 450ms
/// when a restart fails, instead of burning a full second of retries.
const PROBE_RETRIES: u32 = 3;

const PROBE_RETRY_DELAY: Duration = Duration::from_millis(150);

/// Process-wide lock that serialises writes to the `JACK_DEFAULT_SERVER`
/// environment variable. Shared with `build_jack_direct_chain` so the client
/// creation inside the chain runtime respects the same serialisation. Scope:
/// this lifts off the old `JACK_CONNECT_LOCK` static in `lib.rs` verbatim.
/// When Phase 3 collapses all client creation into the supervisor, this
/// becomes an instance field and this `static` goes away.
pub(crate) static JACK_DEFAULT_SERVER_LOCK: Mutex<()> = Mutex::new(());

/// Probe a running named JACK server for its metadata without any caching.
/// Shared between `LiveJackBackend::probe_meta` (for supervisor transitions)
/// and free callers like `jack_enumerate_input_devices` that need to query
/// port counts for device enumeration without instantiating a full
/// supervisor.
///
/// Retries the libjack connection up to [`PROBE_RETRIES`] times — the UNIX
/// socket appears before the shm segments finish initialising, so the first
/// `Client::new` immediately after spawn often fails with "Cannot open shm
/// segment".
pub(crate) fn probe_server_meta(name: &ServerName) -> Result<JackMeta> {
    let _lock = JACK_DEFAULT_SERVER_LOCK.lock().unwrap();
    // SAFETY: lock serialises access to the env var.
    std::env::set_var("JACK_DEFAULT_SERVER", name.as_str());

    let mut last_err: Option<jack::Error> = None;
    let mut client_and_status = None;
    for attempt in 0..PROBE_RETRIES {
        match jack::Client::new("openrig_meta", jack::ClientOptions::NO_START_SERVER) {
            Ok(cs) => {
                client_and_status = Some(cs);
                break;
            }
            Err(e) => {
                if attempt + 1 < PROBE_RETRIES {
                    log::debug!(
                        "probe_server_meta: '{}' attempt {} failed ({:?})",
                        name,
                        attempt + 1,
                        e
                    );
                    std::thread::sleep(PROBE_RETRY_DELAY);
                }
                last_err = Some(e);
            }
        }
    }
    std::env::remove_var("JACK_DEFAULT_SERVER");

    let (client, _) = client_and_status.ok_or_else(|| {
        anyhow!(
            "failed to connect to JACK server '{}': {:?}",
            name,
            last_err.expect("at least one attempt")
        )
    })?;

    let capture_ports = client.ports(Some("system:capture_"), None, jack::PortFlags::IS_OUTPUT);
    let playback_ports = client.ports(Some("system:playback_"), None, jack::PortFlags::IS_INPUT);
    let meta = JackMeta {
        sample_rate: client.sample_rate() as u32,
        buffer_size: client.buffer_size(),
        capture_port_count: capture_ports.len(),
        playback_port_count: playback_ports.len(),
        hw_name: format!("JACK/{}", name),
    };
    drop(client);
    log::debug!(
        "probe_server_meta: server='{}' sr={} buf={} in={} out={}",
        name,
        meta.sample_rate,
        meta.buffer_size,
        meta.capture_port_count,
        meta.playback_port_count
    );
    Ok(meta)
}
