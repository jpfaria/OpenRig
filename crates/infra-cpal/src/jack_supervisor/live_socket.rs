//! Responsibility: tells the caller when a jackd UNIX socket is up
//!
//! Split out of `live_backend.rs` (#873). The socket lives in `/dev/shm` as
//! `jack_<name>_<uid>_0`; its presence is the only cheap liveness signal we
//! have before libjack is willing to talk to us.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::time::Duration;

use super::types::ServerName;

/// Max time we wait for the jackd UNIX socket to appear after `spawn`.
pub(super) const SOCKET_POLL_TIMEOUT: Duration = Duration::from_secs(8);

/// Polling granularity while waiting for the socket.
const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// After the socket appears, wait this long for shm segments to finish
/// initializing before we allow a client to connect. Without this settling
/// window, the very first `jack::Client::new` returns "Cannot open shm
/// segment" on half of the runs.
pub(super) const POST_SOCKET_SETTLING: Duration = Duration::from_millis(600);

pub(super) fn socket_is_present(name: &ServerName) -> bool {
    let prefix = format!("jack_{}_", name);
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let fname = e.file_name();
                let s = fname.to_string_lossy();
                s.starts_with(&prefix) && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

pub(super) fn any_jack_socket_present() -> bool {
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let fname = e.file_name();
                let s = fname.to_string_lossy();
                s.starts_with("jack_") && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

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
