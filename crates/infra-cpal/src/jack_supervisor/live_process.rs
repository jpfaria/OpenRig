//! Responsibility: reaches a jackd process the backend never spawned
//!
//! Split out of `live_backend.rs` (#873). Adoption is the reason this exists:
//! a jackd started by the launcher (or by the user) has no `Child` handle in
//! our process table, so the only handle we get is the pid on `/proc`.
//!
//! Reading a `/proc/<pid>/cmdline` blob and deciding whether it is OUR server
//! is pure parsing, so it lives in `cmdline_is_jackd_for` and is tested on
//! every platform; only the `/proc` walk and the signal are Linux-only.

use super::types::ServerName;

/// `true` when this `/proc/<pid>/cmdline` blob is a jackd serving `name`.
///
/// `cmdline` arrives NUL-separated, which is what makes the match exact: the
/// server name is delimited by the NUL that follows `-n`, so a server called
/// `rig` never matches a running `rig2`.
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // only the Linux walk calls this; the tests cover it everywhere
)]
pub(super) fn cmdline_is_jackd_for(cmdline: &str, name: &ServerName) -> bool {
    let is_jackd = cmdline.starts_with("jackd\0") || cmdline.starts_with("/usr/bin/jackd\0");
    if !is_jackd {
        return false;
    }
    let target_flag = format!("-n\0{}", name.as_str());
    // The name must END where the match ends — either the blob ends there, or
    // the next byte is the NUL separating it from the following argument.
    // A bare `contains` matched server "rig" against a jackd running "rig2",
    // and this answer picks the PID that `terminate` signals (#873).
    match cmdline.find(&target_flag) {
        Some(at) => {
            let after = at + target_flag.len();
            cmdline.len() == after || cmdline.as_bytes()[after] == 0
        }
        None => false,
    }
}

#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn send_signal(pid: u32, signal: &str) -> bool {
    std::process::Command::new("kill")
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
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn discover_pid_for_server(name: &ServerName) -> Option<u32> {
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
        if cmdline_is_jackd_for(&String::from_utf8_lossy(&cmdline), name) {
            return Some(pid);
        }
    }
    None
}

#[cfg(test)]
#[path = "live_process_tests.rs"]
mod tests;
