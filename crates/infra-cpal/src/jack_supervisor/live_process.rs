//! Responsibility: reaches a jackd process the backend never spawned
//!
//! Split out of `live_backend.rs` (#873). Adoption is the reason this exists:
//! a jackd started by the launcher (or by the user) has no `Child` handle in
//! our process table, so the only handle we get is the pid on `/proc`.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::process::Command;

use super::types::ServerName;

pub(super) fn send_signal(pid: u32, signal: &str) -> bool {
    Command::new("kill")
        .args([signal, &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Discover the PID of a jackd server we didn't spawn ourselves by
/// scanning `/proc/<pid>/cmdline` for the `-n <server_name>` flag. Used
/// by `terminate` on the adoption-of-zombie path — without this we'd be
/// unable to SIGTERM an externally-launched jackd and the supervisor
/// would get stuck in a "socket present, can't spawn" loop.
///
/// Returns None if nothing matches (e.g. jackd died between the socket
/// check and this scan, or the cmdline uses a different argv format).
pub(super) fn discover_pid_for_server(name: &ServerName) -> Option<u32> {
    let target_flag = format!("-n\0{}", name.as_str());
    let entries = std::fs::read_dir("/proc").ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let fname = entry.file_name();
        let s = fname.to_string_lossy();
        if !s.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = s.parse::<u32>() else { continue };
        let cmdline_path = entry.path().join("cmdline");
        let Ok(cmdline) = std::fs::read(&cmdline_path) else {
            continue;
        };
        let cmdline_str = String::from_utf8_lossy(&cmdline);
        // /proc/<pid>/cmdline separates args with NUL bytes.
        let is_jackd =
            cmdline_str.starts_with("jackd\0") || cmdline_str.starts_with("/usr/bin/jackd\0");
        if !is_jackd {
            continue;
        }
        if cmdline_str.contains(&target_flag) {
            return Some(pid);
        }
    }
    None
}
