//! Responsibility: says whether a jackd server is up for a given card.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::Result;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::jack_supervisor;
use domain::AudioDeviceDescriptor;

/// Check if a specific named JACK server is running by looking for its socket.
/// jackd -n <name> creates /dev/shm/jack_<name>_<uid>_0
pub(crate) fn jack_server_is_running_for(server_name: &str) -> bool {
    let prefix = format!("jack_{}_", server_name);
    std::fs::read_dir("/dev/shm")
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with(&prefix) && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}
