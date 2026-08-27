//! Responsibility: implements the `JackBackend` contract on top of a real `jackd` process
//!
//! `LiveJackBackend` owns the process table, the `Child` handles and the
//! reaper threads. The mechanics each transition leans on live next door
//! (#873): `live_shm` clears `/dev/shm`, `live_socket` waits on the socket,
//! `live_stderr` reads the driver failure, `live_process` reaches a jackd we
//! did not spawn, `live_probe` reads the server metadata.
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
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use super::backend::{JackBackend, PostReadyStatus};
use super::types::{JackConfig, JackMeta, ServerName};
use super::{live_process, live_shm, live_socket, live_stderr};

/// Re-exported so callers keep reaching these through `live_backend::` —
/// device enumeration (`chain_resolve`, `usb_proc`) and the direct-chain
/// client builder (`jack_direct`) were written against that path.
pub(crate) use super::live_probe::{probe_server_meta, JACK_DEFAULT_SERVER_LOCK};

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

        // Defense-in-depth: the supervisor is responsible for adopting or
        // terminating a running jackd before asking the backend to spawn.
        // Refuse to proceed if a socket is still present — otherwise our
        // `cleanup_stale_dev_shm` below would delete a live server's UNIX
        // socket + semaphores, orphaning it and causing the subsequent spawn
        // to fail with ALSA "device already in use".
        if live_socket::socket_is_present(name) {
            bail!(
                "LiveJackBackend::spawn refused: a jackd socket for '{}' is still present. \
                 The supervisor must adopt or terminate the running server before spawning.",
                name
            );
        }

        // Remove any lingering sockets/semaphores from a prior jackd for
        // this server — stale semaphores are a common cause of "Broken pipe"
        // on the next startup.
        live_shm::cleanup_stale_dev_shm(name);

        // If no jack server is running at all, nuke ALL jackd-related /dev/shm
        // files — registry, data segments, jack_db dir, etc. After a restart
        // cycle our own process's libjack state can remain tied to stale
        // inodes even though `jack_lsp` externally reaches the new server
        // fine (documented in issue #294 / #308 — "Cannot open shm segment
        // (Invalid argument)" on the first Client::new after restart).
        // Clearing the on-disk state forces libjack to rebuild its cached
        // mappings when the next client opens.
        if !live_socket::any_jack_socket_present() {
            live_shm::nuke_process_wide_jack_shm();
        }

        super::alsa_mixer::set_mixer_unity(config.card_num);

        let stderr_log = live_stderr::stderr_log_path(name);
        let stderr_file = std::fs::File::create(&stderr_log)
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());

        // jackd top-level flag: -n <server_name>
        // ALSA backend flags (after -d alsa): -d hw:N -r SR -p BUF -n PERIODS -i CH -o CH
        // Optional realtime: --realtime -P <rt_priority>
        let mut cmd = Command::new("/usr/bin/jackd");
        if config.realtime {
            cmd.arg("--realtime")
                .args(["-P", &config.rt_priority.to_string()]);
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

        // Pin jackd to the big cores so its RT callback thread shares a
        // scheduling domain with the DSP worker (also pinned). Required when
        // the kernel cmdline has isolcpus=4-7 (issue #310): without an
        // explicit affinity, the child inherits the parent's default mask
        // (which excludes isolated cores under isolcpus), and the JACK RT
        // callback ends up on the little cores — re-introducing the
        // UI-vs-audio contention the isolation was meant to eliminate.
        //
        // sched_setaffinity is async-signal-safe per POSIX, so calling it
        // from the post-fork pre-execve hook is sound.
        let big_cores = crate::cpu_affinity::detect_big_cores();
        if !big_cores.is_empty() {
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(move || {
                    let mut set: libc::cpu_set_t = std::mem::zeroed();
                    for &cpu in &big_cores {
                        libc::CPU_SET(cpu, &mut set);
                    }
                    let _ =
                        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
                    Ok(())
                });
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("failed to launch jackd for '{}': {}", name, e))?;

        let pid = child.id();
        log::info!(
            "LiveJackBackend::spawn: jackd PID {} server='{}'",
            pid,
            name
        );

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
        if !live_socket::wait_for_socket(name) {
            // Log whatever stderr captured so the caller sees the root cause.
            let snippet = live_stderr::read_stderr_snippet(&stderr_log);
            for line in snippet.lines().take(20) {
                log::error!("LiveJackBackend::spawn [{}]: {}", name, line);
            }
            bail!(
                "jackd server '{}' socket did not appear within {:?}",
                name,
                live_socket::SOCKET_POLL_TIMEOUT
            );
        }

        // Post-socket settling window. Without this the very first client
        // connect fails with "Cannot open shm segment".
        std::thread::sleep(live_socket::POST_SOCKET_SETTLING);

        Ok(())
    }

    fn terminate(&mut self, name: &ServerName) -> Result<()> {
        log::info!("LiveJackBackend::terminate: server='{}'", name);
        // Discover the PID via /proc when we didn't spawn the server
        // ourselves (adoption path). A tracked PID always wins — we captured
        // it at spawn time and may have metadata that /proc can't recover.
        let pid = self.servers.get(name).map(|s| s.pid).or_else(|| {
            let discovered = live_process::discover_pid_for_server(name);
            if let Some(p) = discovered {
                log::info!(
                    "LiveJackBackend::terminate: discovered pid {} via /proc scan for '{}'",
                    p,
                    name
                );
            } else {
                log::warn!(
                    "LiveJackBackend::terminate: no tracked pid and /proc scan found nothing for '{}'",
                    name
                );
            }
            discovered
        });
        match pid {
            Some(pid) => {
                log::info!(
                    "LiveJackBackend::terminate: SIGTERM pid {} server='{}'",
                    pid,
                    name
                );
                live_process::send_signal(pid, "-TERM");
            }
            None => {
                log::warn!(
                    "LiveJackBackend::terminate: no pid available for '{}' — best-effort wait",
                    name
                );
            }
        }

        // Poll for the socket to disappear — up to 3s.
        for _ in 0..30 {
            if !live_socket::socket_is_present(name) {
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
            live_process::send_signal(pid, "-KILL");
            std::thread::sleep(Duration::from_millis(200));
        } else {
            // No pid ever resolved — can't signal the process, so the socket
            // we see is either a stale file (harmless) or a live jackd whose
            // cmdline doesn't match our discover_pid_for_server pattern
            // (harmful: cleaning the socket tricks the next spawn into
            // running while the process still holds /dev/snd/*). Return Err
            // so the supervisor surfaces a real error instead of entering a
            // respawn loop against a jackd we can't touch.
            if live_socket::socket_is_present(name) {
                bail!(
                    "LiveJackBackend::terminate refused to clear socket for '{}' — no PID \
                     could be discovered and the socket is still present. This usually means a \
                     jackd process was started outside this openrig binary with a cmdline we \
                     can't recognise. Stop it manually (e.g. `pkill jackd`) and retry.",
                    name
                );
            }
        }

        // After SIGKILL the kernel leaves any shm segments the process had
        // open hanging around (unlike AF_UNIX sockets). Clean them up so the
        // next `spawn` doesn't trip its own safety check on a zombie socket.
        if live_socket::socket_is_present(name) {
            log::warn!(
                "LiveJackBackend::terminate: socket still present after kill — removing stale files"
            );
            live_shm::cleanup_stale_dev_shm(name);
            // Verify cleanup actually worked — if the file reappears (some
            // other process races in, e.g. another jackd with the same -n)
            // we mustn't pretend termination succeeded.
            if live_socket::socket_is_present(name) {
                bail!(
                    "LiveJackBackend::terminate: '{}' socket persists after cleanup. \
                     A non-supervised jackd may be racing against us.",
                    name
                );
            }
        }
        Ok(())
    }

    fn probe_meta(&mut self, name: &ServerName) -> Result<JackMeta> {
        probe_server_meta(name)
    }

    fn is_socket_present(&self, name: &ServerName) -> bool {
        live_socket::socket_is_present(name)
    }

    fn post_ready_status(&mut self, name: &ServerName) -> PostReadyStatus {
        // Check 1: did the socket vanish during the settling window? This is
        // the definitive signal that jackd died right after startup.
        if !live_socket::socket_is_present(name) {
            return PostReadyStatus::SocketVanished;
        }
        // Check 2: did stderr pick up an ALSA driver failure marker? If yes,
        // we report a DriverFailure even though the socket is still there,
        // because the next probe_meta will just hang or error out.
        let path = self
            .servers
            .get(name)
            .map(|s| s.stderr_log.clone())
            .unwrap_or_else(|| live_stderr::stderr_log_path(name));
        if let Some(marker) = live_stderr::stderr_has_driver_failure(&path) {
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
#[path = "live_backend_tests.rs"]
mod tests;
