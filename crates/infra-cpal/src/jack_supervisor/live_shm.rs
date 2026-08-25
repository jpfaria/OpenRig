//! Responsibility: removes the shared-memory files a dead jackd leaves behind
//!
//! Split out of `live_backend.rs` (#873). Both entry points are best-effort:
//! a file we fail to remove is logged by its absence from the log line, never
//! escalated — the caller is about to spawn either way.

#![cfg(all(target_os = "linux", feature = "jack"))]

use super::types::ServerName;

/// Best-effort cleanup of stale sockets + semaphores from a prior run of
/// `jackd -n <name>`. Stale semaphores specifically cause "Broken pipe"
/// on the next startup attempt — this mirrors the behaviour of the
/// previous `launch_jackd` prelude.
pub(super) fn cleanup_stale_dev_shm(name: &ServerName) {
    let socket_prefix = format!("jack_{}_", name);
    let sem_infix = format!("_{}_", name);
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for entry in entries.filter_map(|e| e.ok()) {
            let fname = entry.file_name();
            let s = fname.to_string_lossy();
            let stale = s.starts_with(&socket_prefix)
                || (s.starts_with("jack_sem.") && s.contains(&*sem_infix));
            if stale {
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
pub(super) fn nuke_process_wide_jack_shm() {
    let Ok(entries) = std::fs::read_dir("/dev/shm") else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let fname = entry.file_name();
        let s = fname.to_string_lossy();
        // Match every jack-* / jackdb* / jack_db* variant libjack
        // and jackd create. "jack_<name>_*_0" sockets are already
        // handled by cleanup_stale_dev_shm; here we widen to the
        // global files.
        let is_jack = s == "jack-shm-registry"
            || s.starts_with("jack-")
            || s.starts_with("jackdb_")
            || s.starts_with("jack_db");
        if !is_jack {
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
