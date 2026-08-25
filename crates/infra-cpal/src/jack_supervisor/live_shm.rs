//! Responsibility: removes the shared-memory files a dead jackd leaves behind
//!
//! Split out of `live_backend.rs` (#873). Both entry points are best-effort:
//! a file we fail to remove is logged by its absence from the log line, never
//! escalated — the caller is about to spawn either way.
//!
//! Deciding WHICH name is garbage is pure string work, so it lives in
//! `is_stale_entry` / `is_process_wide_entry` and is tested on every platform.
//! Only the `/dev/shm` walk itself is Linux-only.

use super::types::ServerName;

/// `true` when this `/dev/shm` entry belongs to a previous run of
/// `jackd -n <name>` — its socket, or one of its semaphores. Stale semaphores
/// are what make the next startup fail with "Broken pipe".
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // only the Linux walk calls this; the tests cover it everywhere
)]
pub(super) fn is_stale_entry(name: &ServerName, entry: &str) -> bool {
    let socket_prefix = format!("jack_{}_", name);
    let sem_infix = format!("_{}_", name);
    entry.starts_with(&socket_prefix)
        || (entry.starts_with("jack_sem.") && entry.contains(&sem_infix))
}

/// `true` when this `/dev/shm` entry is one of libjack's process-wide files —
/// the registry, the data segments, the jack_db directory. Global across
/// servers, so removing one is only safe when no jackd is running at all.
#[cfg_attr(
    not(all(target_os = "linux", feature = "jack")),
    allow(dead_code) // only the Linux walk calls this; the tests cover it everywhere
)]
pub(super) fn is_process_wide_entry(entry: &str) -> bool {
    entry == "jack-shm-registry"
        || entry.starts_with("jack-")
        || entry.starts_with("jackdb_")
        || entry.starts_with("jack_db")
}

/// Best-effort cleanup of stale sockets + semaphores from a prior run of
/// `jackd -n <name>`. This mirrors the behaviour of the previous
/// `launch_jackd` prelude.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn cleanup_stale_dev_shm(name: &ServerName) {
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name();
            let s = fname.to_string_lossy();
            if is_stale_entry(name, &s) {
                let _ = std::fs::remove_file(entry.path());
                log::info!("LiveJackBackend: removed stale /dev/shm entry {}", s);
            }
        }
    }
}

/// Aggressive cleanup for the "no jackd running anywhere" case — removes
/// the process-wide shm registry + data segments + jack_db directory.
/// After a jackd restart cycle, libjack clients in our own process can
/// end up stuck on stale inode handles even though external tools
/// (`jack_lsp`) reach the new server fine. Removing these files before
/// the next spawn forces libjack to rebuild its cached mappings on the
/// next `Client::new`.
///
/// Only safe to call when NO jackd server of any name is running — the
/// files are global across servers.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub(super) fn nuke_process_wide_jack_shm() {
    let Ok(entries) = std::fs::read_dir("/dev/shm") else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let fname = entry.file_name();
        let s = fname.to_string_lossy();
        if !is_process_wide_entry(&s) {
            continue;
        }
        let path = entry.path();
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path).is_ok()
        } else {
            std::fs::remove_file(&path).is_ok()
        };
        if removed {
            log::info!("LiveJackBackend: nuked process-wide shm entry {}", s);
        }
    }
}

#[cfg(test)]
#[path = "live_shm_tests.rs"]
mod tests;
