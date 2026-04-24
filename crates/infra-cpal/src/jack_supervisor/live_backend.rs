//! `LiveJackBackend` — real [`JackBackend`] implementation backed by `jackd`
//! and the `jack` crate. The code here is the former set of free functions
//! (`launch_jackd`, `stop_jackd_for`, `jack_meta_for`) reorganized into a
//! single struct that owns its process table, its `Child` handles, its reaper
//! threads and its post-ready stderr logs. No module-level `static` state.
//!
//! The backend only exists on Linux+jack — the `ensure_jack_running`/
//! `stop_jackd_for` contract is Linux-specific (ALSA, `/dev/shm` sockets,
//! `/proc/asound` probing). On macOS and Windows the supervisor still works
//! (the types + state machine are platform-agnostic); only this backend is
//! gated.

#![cfg(all(target_os = "linux", feature = "jack"))]

use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::Duration;

use super::backend::{JackBackend, PostReadyStatus};
use super::types::{JackConfig, JackMeta, ServerName};

/// Max time we wait for the jackd UNIX socket to appear after `spawn`.
const SOCKET_POLL_TIMEOUT: Duration = Duration::from_secs(8);

/// Polling granularity while waiting for the socket.
const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// After the socket appears, wait this long for shm segments to finish
/// initializing before we allow a client to connect. Without this settling
/// window, the very first `jack::Client::new` returns "Cannot open shm
/// segment" on half of the runs.
const POST_SOCKET_SETTLING: Duration = Duration::from_millis(600);

/// jackd exits with non-zero status if ALSA refuses to open the device at
/// the requested buffer size. This pattern matches the stderr messages we
/// recognise as definitive driver failures.
const DRIVER_FAILURE_MARKERS: &[&str] = &[
    "Broken pipe",
    "Cannot start driver",
    "Failed to start server",
];

/// Number of client-open retries inside `probe_meta`. The shm segments are
/// still being written when `spawn` returns, so the first `Client::new` often
/// returns "Cannot open shm segment" — retrying always clears that.
const PROBE_RETRIES: u32 = 5;

const PROBE_RETRY_DELAY: Duration = Duration::from_millis(200);

/// Process-wide lock that serialises writes to the `JACK_DEFAULT_SERVER`
/// environment variable. Shared with `build_jack_direct_chain` so the client
/// creation inside the chain runtime respects the same serialisation. Scope:
/// this lifts off the old `JACK_CONNECT_LOCK` static in `lib.rs` verbatim.
/// When Phase 3 collapses all client creation into the supervisor, this
/// becomes an instance field and this `static` goes away.
pub(crate) static JACK_DEFAULT_SERVER_LOCK: Mutex<()> = Mutex::new(());

/// Per-server process bookkeeping owned by the backend.
struct LiveServer {
    pid: u32,
    /// Reaper thread. The thread owns the `Child` handle and blocks on
    /// `wait()`. Dropping the `JoinHandle` does NOT kill the thread — we rely
    /// on the kernel delivering SIGCHLD after `terminate` so the reaper
    /// finishes naturally. `forget` joins the reaper to drain it fully.
    reaper: Option<JoinHandle<()>>,
    stderr_log: PathBuf,
}

/// Default implementation of [`JackBackend`] used in production.
#[derive(Default)]
pub struct LiveJackBackend {
    servers: HashMap<ServerName, LiveServer>,
}

impl LiveJackBackend {
    pub fn new() -> Self {
        Self::default()
    }

    /// Best-effort cleanup of stale sockets + semaphores from a prior run of
    /// `jackd -n <name>`. Stale semaphores specifically cause "Broken pipe"
    /// on the next startup attempt — this mirrors the behaviour of the
    /// previous `launch_jackd` prelude.
    fn cleanup_stale_dev_shm(name: &ServerName) {
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

    fn stderr_log_path(name: &ServerName) -> PathBuf {
        PathBuf::from(format!("/tmp/jackd-{}-stderr.log", name))
    }

    fn socket_is_present(name: &ServerName) -> bool {
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

    fn any_jack_socket_present() -> bool {
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

    fn wait_for_socket(name: &ServerName) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < SOCKET_POLL_TIMEOUT {
            if Self::socket_is_present(name) {
                return true;
            }
            std::thread::sleep(SOCKET_POLL_INTERVAL);
        }
        false
    }

    fn read_stderr_snippet(path: &PathBuf) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn stderr_has_driver_failure(path: &PathBuf) -> Option<String> {
        let content = Self::read_stderr_snippet(path);
        for marker in DRIVER_FAILURE_MARKERS {
            if content.contains(marker) {
                return Some((*marker).to_string());
            }
        }
        None
    }

    fn send_signal(pid: u32, signal: &str) -> bool {
        Command::new("kill")
            .args([signal, &pid.to_string()])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl JackBackend for LiveJackBackend {
    fn spawn(&mut self, name: &ServerName, config: &JackConfig) -> Result<()> {
        log::info!(
            "LiveJackBackend::spawn: server='{}' hw:{} sr={} buf={} nperiods={} rt={} cap={} play={}",
            name,
            config.card_num,
            config.sample_rate,
            config.buffer_size,
            config.nperiods,
            config.realtime,
            config.capture_channels,
            config.playback_channels
        );

        // Remove any lingering sockets/semaphores from a prior jackd for
        // this server — stale semaphores are a common cause of "Broken pipe"
        // on the next startup.
        Self::cleanup_stale_dev_shm(name);

        // If no jack server is running at all, prune the global shm registry
        // too. This is the crumb that accumulates when jackd is SIGKILLed.
        if !Self::any_jack_socket_present() {
            let registry = std::path::Path::new("/dev/shm/jack-shm-registry");
            if registry.exists() {
                log::info!("LiveJackBackend::spawn: removing stale jack-shm-registry");
                let _ = std::fs::remove_file(registry);
            }
        }

        let stderr_log = Self::stderr_log_path(name);
        let stderr_file = std::fs::File::create(&stderr_log)
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());

        // jackd top-level flag: -n <server_name>
        // ALSA backend flags (after -d alsa): -d hw:N -r SR -p BUF -n PERIODS -i CH -o CH
        // Optional realtime: --realtime -P <rt_priority>
        let mut cmd = Command::new("/usr/bin/jackd");
        if config.realtime {
            cmd.arg("--realtime").args(["-P", &config.rt_priority.to_string()]);
        } else {
            cmd.arg("--no-realtime");
        }
        cmd.args([
            "-n",
            name.as_str(),
            "-d",
            "alsa",
            "-d",
            &format!("hw:{}", config.card_num),
            "-r",
            &config.sample_rate.to_string(),
            "-p",
            &config.buffer_size.to_string(),
            "-n",
            &config.nperiods.to_string(),
            "-i",
            &config.capture_channels.to_string(),
            "-o",
            &config.playback_channels.to_string(),
        ])
        .env("JACK_NO_AUDIO_RESERVATION", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr_file);

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to launch jackd for '{}': {}", name, e))?;

        let pid = child.id();
        log::info!("LiveJackBackend::spawn: jackd PID {} server='{}'", pid, name);

        // Reaper thread owns the Child handle for the lifetime of the
        // process. Without a paired wait() the kernel leaves jackd in
        // <defunct> state, and those accumulate every time the user toggles
        // buffer size or sample rate in Settings.
        let reaper_name = name.clone();
        let reaper = std::thread::Builder::new()
            .name(format!("jackd-reaper-{}", name))
            .spawn(move || {
                let result = child.wait();
                log::debug!(
                    "LiveJackBackend reaper: server='{}' pid={} exit={:?}",
                    reaper_name,
                    pid,
                    result
                );
            })
            .map_err(|e| anyhow!("failed to spawn jackd reaper: {}", e))?;

        self.servers.insert(
            name.clone(),
            LiveServer {
                pid,
                reaper: Some(reaper),
                stderr_log: stderr_log.clone(),
            },
        );

        // Wait for the UNIX socket to appear.
        if !Self::wait_for_socket(name) {
            // Log whatever stderr captured so the caller sees the root cause.
            let snippet = Self::read_stderr_snippet(&stderr_log);
            for line in snippet.lines().take(20) {
                log::error!("LiveJackBackend::spawn [{}]: {}", name, line);
            }
            bail!(
                "jackd server '{}' socket did not appear within {:?}",
                name,
                SOCKET_POLL_TIMEOUT
            );
        }

        // Post-socket settling window. Without this the very first client
        // connect fails with "Cannot open shm segment".
        std::thread::sleep(POST_SOCKET_SETTLING);

        Ok(())
    }

    fn terminate(&mut self, name: &ServerName) -> Result<()> {
        log::info!("LiveJackBackend::terminate: server='{}'", name);
        let pid = self.servers.get(name).map(|s| s.pid);
        match pid {
            Some(pid) => {
                log::info!("LiveJackBackend::terminate: SIGTERM pid {} server='{}'", pid, name);
                Self::send_signal(pid, "-TERM");
            }
            None => {
                log::warn!(
                    "LiveJackBackend::terminate: no tracked pid for '{}' — best-effort wait",
                    name
                );
            }
        }

        // Poll for the socket to disappear — up to 3s.
        for _ in 0..30 {
            if !Self::socket_is_present(name) {
                log::info!("LiveJackBackend::terminate: server='{}' socket gone", name);
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if let Some(pid) = pid {
            log::warn!(
                "LiveJackBackend::terminate: pid {} didn't exit after SIGTERM, sending SIGKILL",
                pid
            );
            Self::send_signal(pid, "-KILL");
        }
        std::thread::sleep(Duration::from_millis(200));
        Ok(())
    }

    fn probe_meta(&mut self, name: &ServerName) -> Result<JackMeta> {
        let _lock = JACK_DEFAULT_SERVER_LOCK.lock().unwrap();
        // SAFETY: lock serialises access to the env var.
        std::env::set_var("JACK_DEFAULT_SERVER", name.as_str());

        let mut last_err: Option<jack::Error> = None;
        let mut client_and_status = None;
        for attempt in 0..PROBE_RETRIES {
            match jack::Client::new("openrig_meta", jack::ClientOptions::NO_START_SERVER) {
                Ok(cs) => {
                    client_and_status = Some(cs);
                    break;
                }
                Err(e) => {
                    if attempt + 1 < PROBE_RETRIES {
                        log::debug!(
                            "LiveJackBackend::probe_meta: '{}' attempt {} failed ({:?})",
                            name,
                            attempt + 1,
                            e
                        );
                        std::thread::sleep(PROBE_RETRY_DELAY);
                    }
                    last_err = Some(e);
                }
            }
        }
        std::env::remove_var("JACK_DEFAULT_SERVER");

        let (client, _) = client_and_status.ok_or_else(|| {
            anyhow!(
                "failed to connect to JACK server '{}': {:?}",
                name,
                last_err.expect("at least one attempt")
            )
        })?;

        let capture_ports =
            client.ports(Some("system:capture_"), None, jack::PortFlags::IS_OUTPUT);
        let playback_ports =
            client.ports(Some("system:playback_"), None, jack::PortFlags::IS_INPUT);
        // We cannot reach the USB card's display name from inside the backend
        // without the proc-cache. The supervisor overwrites `hw_name` with
        // the caller-provided card name before exposing the meta; here we
        // just leave a generic placeholder so the contract is honoured.
        let meta = JackMeta {
            sample_rate: client.sample_rate() as u32,
            buffer_size: client.buffer_size(),
            capture_port_count: capture_ports.len(),
            playback_port_count: playback_ports.len(),
            hw_name: format!("JACK/{}", name),
        };
        drop(client);
        log::debug!(
            "LiveJackBackend::probe_meta: server='{}' sr={} buf={} in={} out={}",
            name,
            meta.sample_rate,
            meta.buffer_size,
            meta.capture_port_count,
            meta.playback_port_count
        );
        Ok(meta)
    }

    fn is_socket_present(&self, name: &ServerName) -> bool {
        Self::socket_is_present(name)
    }

    fn post_ready_status(&mut self, name: &ServerName) -> PostReadyStatus {
        // Check 1: did the socket vanish during the settling window? This is
        // the definitive signal that jackd died right after startup.
        if !Self::socket_is_present(name) {
            return PostReadyStatus::SocketVanished;
        }
        // Check 2: did stderr pick up an ALSA driver failure marker? If yes,
        // we report a DriverFailure even though the socket is still there,
        // because the next probe_meta will just hang or error out.
        let path = self
            .servers
            .get(name)
            .map(|s| s.stderr_log.clone())
            .unwrap_or_else(|| Self::stderr_log_path(name));
        if let Some(marker) = Self::stderr_has_driver_failure(&path) {
            return PostReadyStatus::DriverFailure(marker);
        }
        PostReadyStatus::Healthy
    }

    fn forget(&mut self, name: &ServerName) {
        if let Some(mut s) = self.servers.remove(name) {
            // Best-effort join — the reaper should be finishing naturally as
            // jackd exits, but we don't want a dangling thread if the caller
            // is synchronous. A missed join is harmless; the thread detaches.
            if let Some(h) = s.reaper.take() {
                let _ = h.join();
            }
            // Clean up the stderr log so the next spawn for this name starts
            // fresh (prevents false-positive DriverFailure detections).
            let _ = std::fs::remove_file(&s.stderr_log);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests exercise only the pure helpers — no real jackd is launched.
    // Integration tests that actually spawn `jackd` are marked `#[ignore]`
    // and run under `cargo test -- --ignored`.

    #[test]
    fn stderr_log_path_is_scoped_per_server_name() {
        let a = LiveJackBackend::stderr_log_path(&ServerName::from("a"));
        let b = LiveJackBackend::stderr_log_path(&ServerName::from("b"));
        assert_ne!(a, b);
        assert!(a.to_string_lossy().contains("/tmp/jackd-a-"));
    }

    #[test]
    fn stderr_driver_failure_detected_for_known_markers() {
        let tmp = std::env::temp_dir().join("openrig-jack-test-failure.log");
        std::fs::write(&tmp, "xrun\nALSA: could not start playback (Broken pipe)\n").unwrap();
        let marker = LiveJackBackend::stderr_has_driver_failure(&tmp);
        assert_eq!(marker.as_deref(), Some("Broken pipe"));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn stderr_driver_failure_absent_for_benign_content() {
        let tmp = std::env::temp_dir().join("openrig-jack-test-benign.log");
        std::fs::write(&tmp, "JackMessageBuffer:: nothing wrong here\n").unwrap();
        let marker = LiveJackBackend::stderr_has_driver_failure(&tmp);
        assert!(marker.is_none());
        let _ = std::fs::remove_file(&tmp);
    }

    // Integration test — starts a real jackd if one is available on the
    // system. Skipped by default. Run with:
    //   cargo test -p infra-cpal --features jack -- --ignored live_backend
    #[test]
    #[ignore]
    fn live_backend_cold_start_and_shutdown_against_real_jackd() {
        let mut backend = LiveJackBackend::new();
        let name = ServerName::from("openrig-test");
        // Use a harmless card id 99 — this test assumes no such card exists
        // so jackd will fail. The point is to exercise the error path fully.
        let config = JackConfig {
            sample_rate: 48_000,
            buffer_size: 128,
            nperiods: 3,
            realtime: false,
            rt_priority: 70,
            card_num: 99,
            capture_channels: 2,
            playback_channels: 2,
        };
        let err = backend.spawn(&name, &config).unwrap_err();
        assert!(err.to_string().contains(name.as_str()));
        backend.forget(&name);
    }
}
