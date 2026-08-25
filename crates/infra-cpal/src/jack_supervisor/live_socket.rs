//! Responsibility: tells the caller when a jackd UNIX socket is up
//!
//! Split out of `live_backend.rs` (#873). The socket lives in `/dev/shm` as
//! `jack_<name>_<uid>_0`; its presence is the only cheap liveness signal we
//! have before libjack is willing to talk to us.
//!
//! Recognising the socket NAME is pure string work and is tested on every
//! platform; only the `/dev/shm` walk and the wait loop are Linux-only.

use std::time::Duration;

use super::types::ServerName;

/// Max time we wait for the jackd UNIX socket to appear after `spawn`.
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // read by the Linux wait loop only
)]
pub(super) const SOCKET_POLL_TIMEOUT: Duration = Duration::from_secs(8);

/// Polling granularity while waiting for the socket.
#[cfg(all(target_os = "linux", feature = "jack"))]
const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// After the socket appears, wait this long for shm segments to finish
/// initializing before we allow a client to connect. Without this settling
/// window, the very first `jack::Client::new` returns "Cannot open shm
/// segment" on half of the runs.
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // slept on by the Linux spawn path only
)]
pub(super) const POST_SOCKET_SETTLING: Duration = Duration::from_millis(600);

/// `true` when this `/dev/shm` entry is the socket of the named server.
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // only the Linux walk calls this; the tests cover it everywhere
)]
pub(super) fn is_socket_entry(name: &ServerName, entry: &str) -> bool {
    let prefix = format!("jack_{}_", name);
    entry.starts_with(&prefix) && entry.ends_with("_0")
}

/// `true` when this `/dev/shm` entry is the socket of ANY jack server. Used to
/// decide whether the process-wide shm nuke is safe.
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // only the Linux walk calls this; the tests cover it everywhere
)]
pub(super) fn is_any_socket_entry(entry: &str) -> bool {
    entry.starts_with("jack_") && entry.ends_with("_0")
}

#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn socket_is_present(name: &ServerName) -> bool {
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let fname = e.file_name();
                is_socket_entry(name, &fname.to_string_lossy())
            })
        })
        .unwrap_or(false)
}

#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn any_jack_socket_present() -> bool {
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let fname = e.file_name();
                is_any_socket_entry(&fname.to_string_lossy())
            })
        })
        .unwrap_or(false)
}

#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn wait_for_socket(name: &ServerName) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < SOCKET_POLL_TIMEOUT {
        if socket_is_present(name) {
            return true;
        }
        std::thread::sleep(SOCKET_POLL_INTERVAL);
    }
    false
}

#[cfg(test)]
#[path = "live_socket_tests.rs"]
mod tests;
