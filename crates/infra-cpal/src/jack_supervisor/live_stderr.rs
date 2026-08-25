//! Responsibility: recognises the driver failure recorded in a jackd stderr log
//!
//! Split out of `live_backend.rs` (#873). jackd reports an ALSA refusal only
//! on stderr and then exits non-zero, so recognising the message is the only
//! way the supervisor can tell "the driver said no" from "the process died".
//!
//! The recognising is pure and tested on every platform; only reading the file
//! touches the disk.

use std::path::{Path, PathBuf};

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

/// The marker this log records, if any. Pure — `content` is whatever the log
/// held.
pub(super) fn driver_failure_in(content: &str) -> Option<String> {
    DRIVER_FAILURE_MARKERS
        .iter()
        .find(|marker| content.contains(**marker))
        .map(|marker| (*marker).to_string())
}

#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // the Linux backend is the only production caller
)]
pub(super) fn read_stderr_snippet(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // the Linux backend is the only production caller
)]
pub(super) fn stderr_has_driver_failure(path: &Path) -> Option<String> {
    driver_failure_in(&read_stderr_snippet(path))
}

#[cfg(test)]
#[path = "live_stderr_tests.rs"]
mod tests;
