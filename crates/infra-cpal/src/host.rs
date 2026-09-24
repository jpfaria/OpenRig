//! Responsibility: picks the cpal host the app enumerates through.
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
        use crate::windows_host_choice::{choose_windows_host, WindowsHost};
        use cpal::traits::HostTrait;

        crate::com_keepalive::keep_com_alive();

        // cpal reports ASIO as available whether or not a driver is installed
        // (#978), so count what it can actually open.
        let asio = cpal::host_from_id(cpal::HostId::Asio).ok();
        let asio_devices = asio
            .as_ref()
            .and_then(|host| host.devices().ok())
            .map_or(0, |devices| devices.count());
        match (choose_windows_host(asio_devices), asio) {
            (WindowsHost::Asio, Some(host)) => {
                log::info!("Audio host: ASIO ({asio_devices} device(s))");
                return host;
            }
            _ => log::info!("Audio host: WASAPI (no ASIO device found)"),
        }
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
/// /proc/asound and the jack crate. On other platforms, caches a host once
/// so repeated enumerations share the same instance.
///
/// Reusa exatamente `create_host` (a fn que o streaming usa) em vez de
/// `cpal::default_host`: o picker e o streaming PRECISAM enxergar o mesmo
/// conjunto de devices, senão a UI lista o que o WASAPI vê e o stream
/// abre o que o ASIO vê — devices exclusivos do ASIO (Scarlett 2i2 4th Gen
/// em modo exclusive, RME, etc.) somem do picker no Windows (issue #422).
/// macOS/Linux-sem-JACK não têm ASIO, então `create_host` cai no
/// `cpal::default_host` igual antes.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn select_host_for_enumeration() -> &'static cpal::Host {
    static ENUM_HOST: OnceLock<cpal::Host> = OnceLock::new();
    ENUM_HOST.get_or_init(create_host)
}

/// Returns true when the given host is the ASIO host on Windows.
/// ASIO devices report a fixed sample rate and buffer size configured externally
/// via vendor software — project settings must be ignored for those devices.
#[cfg(target_os = "windows")]
pub(crate) fn is_asio_host(host: &cpal::Host) -> bool {
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
