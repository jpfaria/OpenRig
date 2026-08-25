//! Responsibility: recognises the driver failure recorded in a jackd stderr log
//!
//! Split out of `live_backend.rs` (#873). jackd reports an ALSA refusal only
//! on stderr and then exits non-zero, so recognising the message is the only
//! way the supervisor can tell "the driver said no" from "the process died".

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::path::PathBuf;

use super::types::ServerName;

/// jackd exits with non-zero status if ALSA refuses to open the device at
/// the requested buffer size. This pattern matches the stderr messages we
/// recognise as definitive driver failures.
const DRIVER_FAILURE_MARKERS: &[&str] = &[
    "Broken pipe",
    "Cannot start driver",
    "Failed to start server",
];

pub(super) fn stderr_log_path(name: &ServerName) -> PathBuf {
    PathBuf::from(format!("/tmp/jackd-{}-stderr.log", name))
}

pub(super) fn read_stderr_snippet(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

pub(super) fn stderr_has_driver_failure(path: &PathBuf) -> Option<String> {
    let content = read_stderr_snippet(path);
    for marker in DRIVER_FAILURE_MARKERS {
        if content.contains(marker) {
            return Some((*marker).to_string());
        }
    }
    None
}
