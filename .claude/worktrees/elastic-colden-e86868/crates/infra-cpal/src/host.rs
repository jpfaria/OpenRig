//! Cached CPAL host selection and JACK server detection helpers.
//!
//! - `get_host` / `create_host` (non-JACK platforms): cache the CPAL host
//!   once per process so repeated device enumerations share the same host
//!   instance (issue #194 split — was inline in `lib.rs` and called 8+
//!   times per session).
//! - `select_host_for_enumeration` (non-JACK platforms): same idea, but a
//!   separate cache used only by enumeration paths so they can survive
//!   independently from the streaming host.
//! - `jack_server_is_running` (Linux + jack feature): pure filesystem scan
//!   against `/dev/shm/jack_*_0` — safe from any thread, never opens a
//!   client.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use std::sync::OnceLock;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
static HOST: OnceLock<cpal::Host> = OnceLock::new();

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn get_host() -> &'static cpal::Host {
    HOST.get_or_init(create_host)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn create_host() -> cpal::Host {
    #[cfg(target_os = "windows")]
    {
        for host_id in cpal::available_hosts() {
            if host_id == cpal::HostId::Asio {
                match cpal::host_from_id(host_id) {
                    Ok(host) => {
                        log::info!("Audio host: ASIO");
                        return host;
                    }
                    Err(e) => {
                        log::warn!("ASIO driver found but failed to initialize: {e} — falling back to WASAPI");
                    }
                }
            }
        }
        log::info!("Audio host: WASAPI (no ASIO driver found)");
    }

    cpal::default_host()
}

/// Returns true when at least one JACK server is running.
/// jackd creates a socket at /dev/shm/jack_<name>_<uid>_0 for any server name.
/// Safe to call from the UI thread — pure filesystem scan, no JACK client.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) fn jack_server_is_running() -> bool {
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("jack_") && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

/// Select the CPAL audio host for device enumeration (non-JACK path only).
///
/// On Linux+JACK this function does not exist — all enumeration goes through
/// /proc/asound and the jack crate. On other platforms, caches a default host
/// once so repeated enumerations share the same host instance.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn select_host_for_enumeration() -> &'static cpal::Host {
    static ENUM_HOST: OnceLock<cpal::Host> = OnceLock::new();
    ENUM_HOST.get_or_init(cpal::default_host)
}

/// Returns true when the given host is the ASIO host on Windows.
/// ASIO devices report a fixed sample rate and buffer size configured externally
/// via vendor software — project settings must be ignored for those devices.
#[cfg(target_os = "windows")]
pub(crate) fn is_asio_host(host: &cpal::Host) -> bool {
    use cpal::traits::HostTrait;
    host.id() == cpal::HostId::Asio
}

#[cfg(all(
    not(target_os = "windows"),
    not(all(target_os = "linux", feature = "jack"))
))]
pub(crate) fn is_asio_host(_host: &cpal::Host) -> bool {
    false
}

/// Returns true when the direct JACK backend will be used for audio streaming.
/// This replaces is_jack_host() checks — since we never create a CPAL JACK host,
/// we check JACK availability directly instead of inspecting the host type.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(crate) fn using_jack_direct() -> bool {
    jack_server_is_running()
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn using_jack_direct() -> bool {
    false
}
