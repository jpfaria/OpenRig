use anyhow::{anyhow, bail, Result};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::Context;
use cpal::traits::{DeviceTrait, StreamTrait};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::HostTrait;
use cpal::{BufferSize, SampleFormat, Stream, StreamConfig, SupportedStreamConfig};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::{SupportedBufferSize, SupportedStreamConfigRange};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// Phase 1 scaffold for issue #308 — consumed by RuntimeController in Phase 3.
// Dead-code allow-listed until the call-site migration wires it up.
#[cfg(feature = "jack")]
#[allow(dead_code, unused_imports)]
mod jack_supervisor;

// ── Cached host ─────────────────────────────────────────────────────────────
// select_host() was called 8+ times per session (each creating a new CPAL
// host, which on JACK means a new client connection).  Cache it once.
//
// On Linux+JACK the host is never needed: all device enumeration and streaming
// goes through /proc/asound and the jack crate directly — zero ALSA/cpal.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
static HOST: OnceLock<cpal::Host> = OnceLock::new();

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn get_host() -> &'static cpal::Host {
    HOST.get_or_init(create_host)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn create_host() -> cpal::Host {
    #[cfg(target_os = "windows")]
    {
        for host_id in cpal::available_hosts() {
            if host_id == cpal::HostId::Asio {
                match cpal::host_from_id(host_id) {
                    Ok(host) => {
                        log::info!("Audio host: ASIO");
                        return host;
                    }
                    Err(e) => {
                        log::warn!("ASIO driver found but failed to initialize: {e} — falling back to WASAPI");
                    }
                }
            }
        }
        log::info!("Audio host: WASAPI (no ASIO driver found)");
    }

    cpal::default_host()
}

/// Read the physical hardware name managed by JACK from /proc/asound/cards.
///
/// JACK abstracts hardware so CPAL only exposes "cpal_client_in/out" as device
/// names. This function reads the kernel's card list to find the actual hardware
/// name and returns it for display purposes.
/// Returns None if no USB audio card is found or the file is unreadable.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_hardware_name() -> Option<String> {
    // Uses the serialized /proc cache. Direct /proc/asound/cards reads from
    // multiple callsites can race and destabilize USB audio devices whose
    // firmware mirrors kernel-side control access into the USB interrupt
    // endpoint.
    proc_cache_snapshot()
        .and_then(|s| s.cards.first().map(|c| c.display_name.clone()))
}

/// Returns true when at least one JACK server is running.
/// jackd creates a socket at /dev/shm/jack_<name>_<uid>_0 for any server name.
/// Safe to call from the UI thread — pure filesystem scan, no JACK client.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_server_is_running() -> bool {
    std::fs::read_dir("/dev/shm").ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("jack_") && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

/// Select the CPAL audio host for device enumeration (non-JACK path only).
///
/// On Linux+JACK this function does not exist — all enumeration goes through
/// /proc/asound and the jack crate. On other platforms, caches a default host
/// once so repeated enumerations share the same host instance.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn select_host_for_enumeration() -> &'static cpal::Host {
    static ENUM_HOST: OnceLock<cpal::Host> = OnceLock::new();
    ENUM_HOST.get_or_init(cpal::default_host)
}

/// Cached JACK server metadata.
///
/// Populated by a SINGLE transient JACK client on first access and reused for
/// every enumeration, sample-rate query and chain config resolution until the
/// process exits or jackd is restarted. This eliminates the churn of opening
/// a fresh JACK client on every UI interaction (device picker, block change,
/// chain resync, etc.) — which on fragile USB audio stacks (Rockchip xHCI)
/// has been correlated with xHCI host controller resets and interface disconnects.
#[cfg(all(target_os = "linux", feature = "jack"))]
#[derive(Clone)]
struct JackMeta {
    sample_rate: u32,
    buffer_size: u32,
    capture_port_count: usize,
    playback_port_count: usize,
    hw_name: String,
}

/// Per-server JACK metadata cache. Key = server_name (e.g. "gen", "card1").
#[cfg(all(target_os = "linux", feature = "jack"))]
static JACK_META_CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, JackMeta>>> =
    OnceLock::new();

#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_meta_cache() -> &'static std::sync::Mutex<std::collections::HashMap<String, JackMeta>> {
    JACK_META_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Mutex that serializes JACK_DEFAULT_SERVER env-var manipulation so that
/// concurrent jack_meta_for() calls don't race on the global env.
#[cfg(all(target_os = "linux", feature = "jack"))]
static JACK_CONNECT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Per-server jackd PIDs. Populated by launch_jackd, consumed by stop_jackd_for
/// to kill a specific server's process without disturbing other running jackd
/// instances (multiple interfaces, multiple chains on the same interface, etc).
#[cfg(all(target_os = "linux", feature = "jack"))]
static JACKD_PIDS: OnceLock<std::sync::Mutex<std::collections::HashMap<String, u32>>> =
    OnceLock::new();

#[cfg(all(target_os = "linux", feature = "jack"))]
fn jackd_pids() -> &'static std::sync::Mutex<std::collections::HashMap<String, u32>> {
    JACKD_PIDS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Send a signal to a jackd PID, returning true if the process was still alive
/// and the signal was delivered. Uses the generic `kill` command — no
/// device-specific logic.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn send_signal_to_pid(pid: u32, signal: &str) -> bool {
    std::process::Command::new("kill")
        .args([signal, &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Return cached JACK metadata for a specific named server, connecting once
/// if not yet cached. Uses JACK_DEFAULT_SERVER env var to target the right
/// jackd instance (serialized via JACK_CONNECT_LOCK to avoid races).
///
/// Retries the client connection up to 5× with 200 ms between attempts. Right
/// after jackd spawns, the UNIX socket is visible before the shm segments are
/// fully initialized, so the first Client::new returns "Cannot open shm
/// segment". Without retries, ensure_jack_running misreads this as a zombie
/// jackd and enters a kill/respawn loop every time the user changes device
/// settings. Same retry policy as build_jack_direct_chain.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_meta_for(server_name: &str) -> Result<JackMeta> {
    if let Some(cached) = jack_meta_cache().lock().unwrap().get(server_name).cloned() {
        log::trace!("jack meta: cache hit for server '{}'", server_name);
        return Ok(cached);
    }

    log::debug!("jack meta: cache miss for server '{}', connecting", server_name);
    let _lock = JACK_CONNECT_LOCK.lock().unwrap();

    // Re-check after acquiring lock (another thread may have populated cache).
    if let Some(cached) = jack_meta_cache().lock().unwrap().get(server_name).cloned() {
        return Ok(cached);
    }

    // SAFETY: we hold JACK_CONNECT_LOCK, so no other thread is touching this env var.
    std::env::set_var("JACK_DEFAULT_SERVER", server_name);
    let mut last_err: Option<jack::Error> = None;
    let mut client_and_status = None;
    for attempt in 0..5u32 {
        match jack::Client::new("openrig_meta", jack::ClientOptions::NO_START_SERVER) {
            Ok(cs) => {
                client_and_status = Some(cs);
                break;
            }
            Err(e) => {
                if attempt < 4 {
                    log::debug!(
                        "jack_meta_for '{}' attempt {} failed ({:?}), retrying in 200ms",
                        server_name, attempt + 1, e
                    );
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                last_err = Some(e);
            }
        }
    }
    std::env::remove_var("JACK_DEFAULT_SERVER");

    let (client, _) = client_and_status.ok_or_else(|| {
        anyhow!(
            "failed to connect to JACK server '{}': {:?}",
            server_name,
            last_err.expect("at least one attempt failed")
        )
    })?;

    let capture_ports = client.ports(Some("system:capture_"), None, jack::PortFlags::IS_OUTPUT);
    let playback_ports = client.ports(Some("system:playback_"), None, jack::PortFlags::IS_INPUT);
    let hw_name = jack_hardware_name().unwrap_or_else(|| format!("JACK/{}", server_name));
    let meta = JackMeta {
        sample_rate: client.sample_rate() as u32,
        buffer_size: client.buffer_size(),
        capture_port_count: capture_ports.len(),
        playback_port_count: playback_ports.len(),
        hw_name,
    };
    drop(client);

    log::debug!(
        "jack meta: cached server='{}' sr={} buf={} in={} out={} hw='{}'",
        server_name, meta.sample_rate, meta.buffer_size,
        meta.capture_port_count, meta.playback_port_count, meta.hw_name
    );
    jack_meta_cache().lock().unwrap().insert(server_name.to_string(), meta.clone());
    Ok(meta)
}

/// Convenience wrapper: connects to the first running JACK server found.
/// Used by legacy call sites that don't yet know a specific server name.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_meta() -> Result<JackMeta> {
    // Try all known running servers (from detected USB cards).
    let cards = detect_all_usb_audio_cards();
    for card in &cards {
        if jack_server_is_running_for(&card.server_name) {
            return jack_meta_for(&card.server_name);
        }
    }
    // Fallback: try "default" (legacy single-server mode)
    if jack_server_is_running_for("default") {
        return jack_meta_for("default");
    }
    bail!("no running JACK server found")
}

/// Invalidate cached metadata for a specific server.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn invalidate_jack_meta_cache_for(server_name: &str) {
    if let Ok(mut cache) = jack_meta_cache().lock() {
        if cache.remove(server_name).is_some() {
            log::info!("jack meta: cache invalidated for server '{}'", server_name);
        }
    }
}

/// Invalidate cached metadata for all servers.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn invalidate_jack_meta_cache() {
    if let Ok(mut cache) = jack_meta_cache().lock() {
        if !cache.is_empty() {
            log::info!("jack meta: all caches invalidated");
            cache.clear();
        }
    }
}

/// Represents a USB audio card detected in /proc/asound/cards.
#[cfg(all(target_os = "linux", feature = "jack"))]
#[derive(Debug, Clone)]
struct UsbAudioCard {
    /// ALSA card number, e.g. "1"
    card_num: String,
    /// JACK server name derived from bracket name, e.g. "gen" for [Gen]
    server_name: String,
    /// Human-readable name, e.g. "USB Audio Interface"
    display_name: String,
    /// device_id used in chain I/O blocks, e.g. "jack:gen"
    device_id: String,
    /// Capture channel count read from /proc/asound/card{N}/stream0.
    /// Read exactly once when the card is first observed on the USB bus.
    capture_channels: u32,
    /// Playback channel count read from /proc/asound/card{N}/stream0.
    /// Read exactly once when the card is first observed on the USB bus.
    playback_channels: u32,
}

/// Derive a safe JACK server name from the ALSA bracket identifier.
/// E.g. "[Gen            ]" → "gen", "[Card1          ]" → "card1"
#[cfg(all(target_os = "linux", feature = "jack"))]
fn server_name_from_bracket(bracket: &str) -> String {
    bracket
        .trim_matches(|c: char| c == '[' || c == ']' || c.is_whitespace())
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

// ── Serialized /proc/asound cache ───────────────────────────────────────────
// On RK3588 + Scarlett 4th Gen, concurrent reads of /proc/asound/{cards,card*/
// stream0} trigger scarlett2_notify 0x20000000 which freezes the device. All
// reads must be serialized through a single mutex; requests that arrive while
// another refresh is in progress return cached data instead of queueing.

#[cfg(all(target_os = "linux", feature = "jack"))]
const PROC_CACHE_TTL: Duration = Duration::from_secs(10);

#[cfg(all(target_os = "linux", feature = "jack"))]
#[derive(Clone)]
struct ProcAsoundSnapshot {
    cards: Vec<UsbAudioCard>,
    fetched_at: Instant,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
static PROC_CACHE: Mutex<Option<ProcAsoundSnapshot>> = Mutex::new(None);

#[cfg(all(target_os = "linux", feature = "jack"))]
static PROC_REFRESH_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(target_os = "linux", feature = "jack"))]
fn invalidate_proc_cache() {
    *PROC_CACHE.lock().unwrap() = None;
}

#[cfg(all(target_os = "linux", feature = "jack"))]
fn proc_cache_is_fresh() -> bool {
    PROC_CACHE
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.fetched_at.elapsed() < PROC_CACHE_TTL)
        .unwrap_or(false)
}

/// Read and parse /proc/asound/cards + card{N}/stream0 for each USB card.
/// Direct filesystem I/O — only called under PROC_REFRESH_LOCK.
// Process-lifetime registry of channel counts per physical card. Keyed by
// display_name (e.g. "Scarlett 2i2 4th Gen at usb-xhci-hcd.3.auto-1, ...")
// so the lookup is stable across plug/unplug cycles. stream0 is read exactly
// ONCE per distinct physical card that the app ever observes; the value is
// kept in memory forever. Prevents the Scarlett firmware from seeing repeated
// stream0 reads, which cause scarlett2_notify 0x20000000 → freeze.
#[cfg(all(target_os = "linux", feature = "jack"))]
static CARD_CHANNELS_REGISTRY: Mutex<Option<std::collections::HashMap<String, (u32, u32)>>> =
    Mutex::new(None);

#[cfg(all(target_os = "linux", feature = "jack"))]
fn lookup_or_cache_card_channels(display_name: &str, card_num: &str) -> (u32, u32) {
    {
        let guard = CARD_CHANNELS_REGISTRY.lock().unwrap();
        if let Some(map) = guard.as_ref() {
            if let Some(&ch) = map.get(display_name) {
                return ch;
            }
        }
    }
    // First time we see this display_name — read stream0 once and store forever.
    let ch = read_card_channels_raw(card_num);
    let mut guard = CARD_CHANNELS_REGISTRY.lock().unwrap();
    let map = guard.get_or_insert_with(std::collections::HashMap::new);
    map.insert(display_name.to_string(), ch);
    log::info!(
        "[CARD-REGISTRY] learned '{}' → capture={} playback={}",
        display_name, ch.0, ch.1
    );
    ch
}

#[cfg(all(target_os = "linux", feature = "jack"))]
fn read_proc_asound_snapshot() -> ProcAsoundSnapshot {
    log::trace!("[PROC-CACHE] >>> OPEN /proc/asound/cards");
    let content = std::fs::read_to_string("/proc/asound/cards").unwrap_or_default();
    let mut cards = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        let Some(first) = trimmed.chars().next() else { continue };
        if !first.is_ascii_digit() {
            continue;
        }
        if !(trimmed.contains("USB-Audio") || trimmed.contains("USB Audio")) {
            continue;
        }
        let card_num = match trimmed.split_whitespace().next() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let bracket = match (trimmed.find('['), trimmed.find(']')) {
            (Some(a), Some(b)) if b > a => trimmed[a..=b].to_string(),
            _ => format!("[card{}]", card_num),
        };
        let server_name = server_name_from_bracket(&bracket);
        let display_name = if let Some(pos) = trimmed.find(" - ") {
            trimmed[pos + 3..].trim().to_string()
        } else {
            format!("USB Audio Card {}", card_num)
        };
        let device_id = format!("jack:{}", server_name);
        let (capture_channels, playback_channels) =
            lookup_or_cache_card_channels(&display_name, &card_num);
        cards.push(UsbAudioCard {
            card_num,
            server_name,
            display_name,
            device_id,
            capture_channels,
            playback_channels,
        });
    }
    ProcAsoundSnapshot {
        cards,
        fetched_at: Instant::now(),
    }
}

/// Non-blocking refresh: if another refresh is already running, skip. The
/// caller who was blocked will simply read the existing cache afterwards.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn try_refresh_proc_cache() {
    let Ok(_guard) = PROC_REFRESH_LOCK.try_lock() else {
        log::debug!("[PROC-CACHE] try_refresh SKIPPED (another refresh in progress)");
        return;
    };
    if proc_cache_is_fresh() {
        log::debug!("[PROC-CACHE] try_refresh SKIPPED (became fresh while waiting)");
        return;
    }
    let caller = std::panic::Location::caller();
    log::debug!("[PROC-CACHE] REFRESH /proc/asound — triggered from {}:{}", caller.file(), caller.line());
    let snapshot = read_proc_asound_snapshot();
    *PROC_CACHE.lock().unwrap() = Some(snapshot);
}

#[cfg(all(target_os = "linux", feature = "jack"))]
#[track_caller]
fn proc_cache_snapshot() -> Option<ProcAsoundSnapshot> {
    let fresh = proc_cache_is_fresh();
    if !fresh {
        let caller = std::panic::Location::caller();
        log::debug!("[PROC-CACHE] snapshot STALE — caller={}:{}", caller.file(), caller.line());
        try_refresh_proc_cache();
    }
    PROC_CACHE.lock().unwrap().clone()
}

/// Detect all USB audio ALSA cards. Serialized + cached: concurrent callers
/// receive a cached snapshot instead of hammering /proc/asound.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn detect_all_usb_audio_cards() -> Vec<UsbAudioCard> {
    proc_cache_snapshot().map(|s| s.cards).unwrap_or_default()
}

/// Check if a specific named JACK server is running by looking for its socket.
/// jackd -n <name> creates /dev/shm/jack_<name>_<uid>_0
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_server_is_running_for(server_name: &str) -> bool {
    let prefix = format!("jack_{}_", server_name);
    std::fs::read_dir("/dev/shm").ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with(&prefix) && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

/// Direct /proc/asound/card{N}/stream0 read — only called from inside
/// `lookup_or_cache_card_channels` when a new card is first observed.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn read_card_channels_raw(card: &str) -> (u32, u32) {
    let path = format!("/proc/asound/card{}/stream0", card);
    log::trace!("[PROC-CACHE] >>> OPEN {}", path);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            log::warn!("read_card_channels_raw: cannot read {}, using defaults 2/2", path);
            return (2, 2);
        }
    };

    let mut capture_ch: Option<u32> = None;
    let mut playback_ch: Option<u32> = None;
    let mut in_capture = false;
    let mut in_playback = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Capture:") {
            in_capture = true;
            in_playback = false;
        } else if trimmed.starts_with("Playback:") {
            in_playback = true;
            in_capture = false;
        } else if trimmed.starts_with("Channels:") {
            // "Channels: 4" or "Channels: 2"
            if let Some(n) = trimmed
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse::<u32>().ok())
            {
                if in_capture && capture_ch.is_none() {
                    capture_ch = Some(n);
                } else if in_playback && playback_ch.is_none() {
                    playback_ch = Some(n);
                }
            }
        }
    }

    let capture = capture_ch.unwrap_or(2);
    let playback = playback_ch.unwrap_or(2);
    log::info!(
        "read_card_channels_raw: card {} → capture={} playback={}",
        card, capture, playback
    );
    (capture, playback)
}

/// Launch jackd for a specific USB audio card and wait for it to become ready.
/// Uses `jackd -n <server_name>` so each interface gets its own named server.
/// Channel counts are read dynamically from /proc/asound/card{N}/stream0.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn launch_jackd(card: &UsbAudioCard, sample_rate: u32, buffer_size: u32) -> Result<()> {
    // Channel counts come from the card's cached values — learned once from
    // /proc/asound/card{N}/stream0 when the device first appeared and kept in
    // memory for the process lifetime via CARD_CHANNELS_REGISTRY.
    let (capture_ch, playback_ch) = (card.capture_channels, card.playback_channels);

    log::info!(
        "launch_jackd: server='{}' hw:{} sr={} buf={} capture={} playback={}",
        card.server_name, card.card_num, sample_rate, buffer_size, capture_ch, playback_ch
    );

    // Clean up stale sockets and semaphore files left by a previous run for
    // this server. jackd leaves behind jack_<name>_* sockets AND
    // jack_sem.*_<name>_* semaphore files that must be removed before a fresh
    // start — stale semaphores cause "Broken pipe" on the next startup attempt.
    let socket_prefix = format!("jack_{}_", card.server_name);
    let sem_infix = format!("_{}_", card.server_name);
    if let Ok(entries) = std::fs::read_dir("/dev/shm") {
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            let stale = s.starts_with(&socket_prefix)
                || (s.starts_with("jack_sem.") && s.contains(&*sem_infix));
            if stale {
                let _ = std::fs::remove_file(entry.path());
                log::info!("launch_jackd: removed stale file {}", s);
            }
        }
    }

    // Clean up the shared shm registry if no server is running at all.
    if !jack_server_is_running() {
        let shm_registry = std::path::Path::new("/dev/shm/jack-shm-registry");
        if shm_registry.exists() {
            log::info!("launch_jackd: removing stale jack-shm-registry");
            let _ = std::fs::remove_file(shm_registry);
        }
    }

    let stderr_log = format!("/tmp/jackd-{}-stderr.log", card.server_name);
    let stderr_file = std::fs::File::create(&stderr_log)
        .map(std::process::Stdio::from)
        .unwrap_or_else(|_| std::process::Stdio::null());

    // jackd top-level flag: -n <server_name>
    // ALSA backend flags (after -d alsa): -d hw:N -r SR -p BUF -n PERIODS -i CH -o CH
    // Note: -n appears twice — first is jackd server name, second is ALSA nperiods.
    let mut child = std::process::Command::new("/usr/bin/jackd")
        .args([
            "-n", &card.server_name,
            "-d", "alsa",
            "-d", &format!("hw:{}", card.card_num),
            "-r", &sample_rate.to_string(),
            "-p", &buffer_size.to_string(),
            "-n", "3",
            "-i", &capture_ch.to_string(),
            "-o", &playback_ch.to_string(),
        ])
        .env("JACK_NO_AUDIO_RESERVATION", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(stderr_file)
        .spawn()
        .map_err(|e| anyhow!("failed to launch jackd for '{}': {}", card.server_name, e))?;

    let pid = child.id();
    log::info!(
        "launch_jackd: jackd spawned PID {} for server '{}'",
        pid, card.server_name
    );
    jackd_pids().lock().unwrap().insert(card.server_name.clone(), pid);

    // Reap the child when it eventually exits (SIGTERM from stop_jackd_for,
    // ALSA failure, or user kill). Without this, Command::spawn returned a
    // `Child` handle that nobody ever calls `.wait()` on, so every jackd
    // restart leaves a `<defunct>` entry in ps until the parent process
    // exits — they accumulate across Settings buffer/sample-rate toggles.
    // The detached thread just blocks on wait() and reaps silently.
    let server_name_for_reaper = card.server_name.clone();
    std::thread::Builder::new()
        .name(format!("jackd-reaper-{}", server_name_for_reaper))
        .spawn(move || {
            let result = child.wait();
            log::debug!(
                "launch_jackd: reaper for server '{}' PID {} saw exit ({:?})",
                server_name_for_reaper, pid, result
            );
        })
        .map_err(|e| anyhow!("failed to spawn jackd reaper thread: {}", e))?;

    // Wait up to 8 seconds for the server socket to appear, then add a fixed
    // 600ms delay for the shm segments to be fully initialized.
    // The UNIX socket appears before the shm is ready — without this delay,
    // the first client connection attempt fails with "Cannot open shm segment".
    for i in 0..80 {
        if jack_server_is_running_for(&card.server_name) {
            log::info!(
                "launch_jackd: server '{}' socket ready after {}ms, waiting 600ms for shm",
                card.server_name, i * 100
            );
            std::thread::sleep(std::time::Duration::from_millis(600));

            // Issue #294: validate jackd is still alive AFTER the shm window.
            // Scenario seen with Q26 at buffer_size=64 on RK3588: jackd opens
            // its UNIX socket, we log "socket ready", sleep 600ms, log "ready"
            // and return Ok — but in that same window ALSA reports "Broken
            // pipe" and jackd exits. The caller then believes the server is
            // healthy, sync_project tears down old chains in anticipation of
            // rebuilding on the new server, and when it tries to connect the
            // new client the socket is gone. Openrig ends up in a retry loop
            // it can never escape without a full service restart.
            //
            // Two cheap checks close the window:
            //   1. Is the socket still present? (jackd exiting cleanly removes
            //      it.)
            //   2. Does stderr show a definitive ALSA/driver failure?
            if !jack_server_is_running_for(&card.server_name) {
                log::warn!(
                    "launch_jackd: server '{}' socket vanished after ready — jackd exited post-startup",
                    card.server_name
                );
                if let Ok(stderr_content) = std::fs::read_to_string(&stderr_log) {
                    for line in stderr_content.lines().take(20) {
                        log::error!("launch_jackd [{}]: {}", card.server_name, line);
                    }
                }
                bail!(
                    "jackd server '{}' exited right after startup (likely ALSA/driver failure — check buffer size vs device)",
                    card.server_name
                );
            }
            if let Ok(stderr_content) = std::fs::read_to_string(&stderr_log) {
                let failed = stderr_content.contains("Broken pipe")
                    || stderr_content.contains("Cannot start driver")
                    || stderr_content.contains("Failed to start server");
                if failed {
                    log::warn!(
                        "launch_jackd: server '{}' stderr reports driver failure after ready",
                        card.server_name
                    );
                    for line in stderr_content.lines().take(20) {
                        log::error!("launch_jackd [{}]: {}", card.server_name, line);
                    }
                    bail!(
                        "jackd server '{}' reported driver failure during startup (see stderr above)",
                        card.server_name
                    );
                }
            }

            log::info!("launch_jackd: server '{}' ready", card.server_name);
            invalidate_jack_meta_cache_for(&card.server_name);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));

        // After 2 seconds, check stderr for a definitive failure message so we
        // bail out early instead of waiting the full 8 seconds.
        if i == 20 {
            if let Ok(stderr_content) = std::fs::read_to_string(&stderr_log) {
                let failed = stderr_content.contains("Broken pipe")
                    || stderr_content.contains("Cannot start driver")
                    || stderr_content.contains("Failed to start server");
                if failed {
                    log::warn!(
                        "launch_jackd: server '{}' failed early, aborting wait",
                        card.server_name
                    );
                    break;
                }
            }
        }
    }

    // Log stderr to help diagnose the failure.
    if let Ok(stderr_content) = std::fs::read_to_string(&stderr_log) {
        for line in stderr_content.lines().take(20) {
            log::error!("launch_jackd [{}]: {}", card.server_name, line);
        }
    }

    bail!(
        "jackd server '{}' failed to start",
        card.server_name
    )
}

/// Check whether calling `ensure_jack_running(project)` would trigger a
/// stop+restart of any jackd server (config mismatch or zombie server).
///
/// Side-effect-free — used by `sync_project` to decide whether to tear
/// down the in-process `ActiveChainRuntime`s (which each hold a jack
/// `AsyncClient` bound to the current jackd's shm) BEFORE the server is
/// killed. Dropping AsyncClients AFTER the kill leaves the jack-rs
/// library in a confused state where new `jack::Client::new` calls fail
/// with `ClientStatus(FAILURE | SERVER_ERROR)` even though the new jackd
/// is alive and responsive to external clients like `jack_lsp`.
///
/// Returns true when at least one card's running server needs a restart
/// (config mismatch or zombie) OR has no server and needs a first launch.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_restart_would_be_triggered(project: &Project) -> bool {
    let cards = detect_all_usb_audio_cards();
    for card in &cards {
        let (desired_sr, desired_buf) = project
            .device_settings
            .iter()
            .find(|s| s.device_id.0 == card.device_id)
            .map(|s| (s.sample_rate, s.buffer_size_frames))
            .unwrap_or((48000, 64));

        if jack_server_is_running_for(&card.server_name) {
            match jack_meta_for(&card.server_name) {
                Ok(meta) if meta.sample_rate == desired_sr && meta.buffer_size == desired_buf => {
                    // Healthy and matches — no restart needed for this card.
                    continue;
                }
                _ => {
                    // Either mismatch or unresponsive (zombie) — restart path
                    // will fire.
                    return true;
                }
            }
        } else {
            // No server running for this card — launch path will fire, but
            // since there's no AsyncClient to tear down, no need to force
            // the "restart-aware" branch.
            //
            // (Exception: if we ever run multiple cards and one has a
            // running server with an AsyncClient while another needs first
            // launch, the caller should still tear down to avoid the
            // jack-rs global-state race. Signal by returning true.)
            return true;
        }
    }
    false
}

/// Ensure JACK is running for every connected USB audio interface.
/// For each interface, starts a named jackd server if not already running.
/// If sr/buf changed, restarts the affected server.
///
/// Returns true if any server was (re)started.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn ensure_jack_running(project: &Project) -> Result<bool> {
    let cards = detect_all_usb_audio_cards();
    if cards.is_empty() {
        bail!("no USB audio interface found — connect a device before starting audio");
    }

    let mut any_started = false;
    for card in &cards {
        // Sample rate and buffer size are per-device, not per-project. Look up
        // the settings for this specific card by device_id; fall back to
        // sensible defaults if the user hasn't configured this device yet.
        let (desired_sr, desired_buf) = project
            .device_settings
            .iter()
            .find(|s| s.device_id.0 == card.device_id)
            .map(|s| (s.sample_rate, s.buffer_size_frames))
            .unwrap_or((48000, 64));

        if jack_server_is_running_for(&card.server_name) {
            match jack_meta_for(&card.server_name) {
                Ok(meta) if meta.sample_rate == desired_sr && meta.buffer_size == desired_buf => {
                    log::debug!(
                        "ensure_jack_running: server '{}' already running with correct config",
                        card.server_name
                    );
                    continue;
                }
                Ok(meta) => {
                    log::info!(
                        "ensure_jack_running: server '{}' config mismatch (sr={} buf={} → sr={} buf={}), restarting",
                        card.server_name, meta.sample_rate, meta.buffer_size, desired_sr, desired_buf
                    );
                    stop_jackd_for(&card.server_name)?;
                }
                Err(e) => {
                    // Socket exists but server is unresponsive — jackd entered zombie
                    // state after the ALSA device disconnected. Kill it before restarting.
                    log::warn!(
                        "ensure_jack_running: server '{}' socket present but unresponsive ({}), killing zombie",
                        card.server_name, e
                    );
                    let _ = stop_jackd_for(&card.server_name);
                }
            }
        }

        // Retry up to 3 times — the ALSA device occasionally needs a moment to
        // settle after a USB reconnect or a previous jackd exit. "Broken pipe"
        // on the first attempt is normal; the second or third attempt succeeds.
        let mut last_err = anyhow!("no attempt made");
        let mut started = false;
        for attempt in 1u32..=3 {
            match launch_jackd(card, desired_sr, desired_buf) {
                Ok(()) => {
                    started = true;
                    break;
                }
                Err(e) => {
                    log::warn!(
                        "ensure_jack_running: attempt {}/3 for '{}' failed: {}",
                        attempt, card.server_name, e
                    );
                    last_err = e;
                    if attempt < 3 {
                        // Kill only this server's orphan jackd (if one was spawned
                        // and its socket did not appear). Do NOT killall — other
                        // interfaces or chains may have healthy jackd processes.
                        let _ = stop_jackd_for(&card.server_name);
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                }
            }
        }
        if !started {
            return Err(last_err);
        }
        any_started = true;
    }

    if any_started {
        invalidate_device_cache();
    }
    Ok(any_started)
}

/// Stop the jackd process owning `server_name` without disturbing any other
/// jackd instances (other interfaces, other chains sharing the same interface
/// on different channels, etc).
///
/// Kills the specific PID recorded in `JACKD_PIDS` at launch time — never
/// uses `killall`. If the PID is unknown (e.g. jackd was started by another
/// process) we still best-effort wait for the socket to clear, but do not
/// affect other servers.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn stop_jackd_for(server_name: &str) -> Result<()> {
    log::info!("stop_jackd_for: stopping server '{}'", server_name);

    let pid = jackd_pids().lock().unwrap().get(server_name).copied();
    match pid {
        Some(pid) => {
            log::info!("stop_jackd_for: sending SIGTERM to PID {} (server '{}')", pid, server_name);
            send_signal_to_pid(pid, "-TERM");
        }
        None => {
            log::warn!(
                "stop_jackd_for: no tracked PID for server '{}' — waiting for socket to clear",
                server_name
            );
        }
    }

    for _ in 0..30 {
        if !jack_server_is_running_for(server_name) {
            jackd_pids().lock().unwrap().remove(server_name);
            invalidate_jack_meta_cache_for(server_name);
            log::info!("stop_jackd_for: server '{}' stopped", server_name);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    if let Some(pid) = pid {
        log::warn!("stop_jackd_for: PID {} didn't exit after SIGTERM, sending SIGKILL", pid);
        send_signal_to_pid(pid, "-KILL");
    }
    std::thread::sleep(std::time::Duration::from_millis(200));
    jackd_pids().lock().unwrap().remove(server_name);
    invalidate_jack_meta_cache_for(server_name);
    Ok(())
}

/// Enumerate input devices via JACK — one entry per running named JACK server.
/// device_id is "jack:<server_name>" (e.g. "jack:gen", "jack:card1").
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_enumerate_input_devices() -> Result<Vec<AudioDeviceDescriptor>> {
    let cards = detect_all_usb_audio_cards();
    let mut devices = Vec::new();
    for card in &cards {
        if !jack_server_is_running_for(&card.server_name) {
            continue;
        }
        if let Ok(meta) = jack_meta_for(&card.server_name) {
            if meta.capture_port_count > 0 {
                devices.push(AudioDeviceDescriptor {
                    id: card.device_id.clone(),
                    name: format!("{} (JACK)", card.display_name),
                    channels: meta.capture_port_count,
                });
            }
        }
    }
    Ok(devices)
}

/// Enumerate output devices via JACK — one entry per running named JACK server.
/// device_id is "jack:<server_name>" (e.g. "jack:gen", "jack:card1").
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_enumerate_output_devices() -> Result<Vec<AudioDeviceDescriptor>> {
    let cards = detect_all_usb_audio_cards();
    let mut devices = Vec::new();
    for card in &cards {
        if !jack_server_is_running_for(&card.server_name) {
            continue;
        }
        if let Ok(meta) = jack_meta_for(&card.server_name) {
            if meta.playback_port_count > 0 {
                devices.push(AudioDeviceDescriptor {
                    id: card.device_id.clone(),
                    name: format!("{} (JACK)", card.display_name),
                    channels: meta.playback_port_count,
                });
            }
        }
    }
    Ok(devices)
}

/// Returns true when the direct JACK backend will be used for audio streaming.
/// This replaces is_jack_host() checks — since we never create a CPAL JACK host,
/// we check JACK availability directly instead of inspecting the host type.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn using_jack_direct() -> bool {
    jack_server_is_running()
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn using_jack_direct() -> bool {
    false
}

/// Returns true when the given host is the ASIO host on Windows.
/// ASIO devices report a fixed sample rate and buffer size configured externally
/// via vendor software — project settings must be ignored for those devices.
#[cfg(target_os = "windows")]
fn is_asio_host(host: &cpal::Host) -> bool {
    host.id() == cpal::HostId::Asio
}

#[cfg(all(not(target_os = "windows"), not(all(target_os = "linux", feature = "jack"))))]
fn is_asio_host(_host: &cpal::Host) -> bool {
    false
}

// is_jack_host() removed — CPAL JACK host is never created.
// Use using_jack_direct() to check if the direct JACK backend is active.

use domain::ids::ChainId;
use engine::runtime::{
    elastic_target_for_buffer, process_input_f32, process_output_f32, ChainRuntimeState,
    RuntimeGraph,
};
use engine;

/// Backend-specific multiplier for the elastic buffer target.
/// JACK uses a worker-thread DSP path on Linux; non-RT scheduling jitter
/// needs more headroom than direct CPAL callbacks.
#[cfg(all(target_os = "linux", feature = "jack"))]
const ELASTIC_MULTIPLIER: u8 = 8;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
const ELASTIC_MULTIPLIER: u8 = 2;

/// Multiplier used for the elastic target of a regular output route.
/// See `ELASTIC_MULTIPLIER` for the per-backend rationale.
const ELASTIC_MULTIPLIER_REGULAR: u8 = ELASTIC_MULTIPLIER;
/// Multiplier used for the elastic target of an Insert block's *send*
/// endpoint. The main chain's elastic buffer already absorbs upstream
/// jitter before the signal reaches the insert send, and the external
/// hardware on the other side has its own driver buffering. Keeping the
/// send's elastic at the default multiplier would be pure redundancy
/// and roughly doubles the insert's round-trip latency; `1` trims that
/// overhead while the shared `ELASTIC_TARGET_FLOOR` prevents pathologic
/// sizing for tiny device buffers.
const ELASTIC_MULTIPLIER_INSERT_SEND: u8 = 1;

/// Compute per-output elastic targets for a chain. Regular outputs use
/// the backend's default multiplier; Insert send endpoints use a leaner
/// multiplier to avoid doubling the round-trip latency of the external
/// effect loop. The order of the returned Vec matches
/// `ResolvedChainAudioConfig::outputs`, which places regular outputs
/// first and Insert sends last (mirroring `effective_outputs`).
fn compute_elastic_targets_for_chain(
    chain: &Chain,
    resolved: &ResolvedChainAudioConfig,
) -> Vec<usize> {
    let regular_output_count: usize = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob.entries.len()),
            _ => None,
        })
        .sum();
    resolved
        .outputs
        .iter()
        .enumerate()
        .map(|(idx, out)| {
            let buf = resolved_output_buffer_size_frames(out);
            let multiplier = if idx >= regular_output_count {
                ELASTIC_MULTIPLIER_INSERT_SEND
            } else {
                ELASTIC_MULTIPLIER_REGULAR
            };
            elastic_target_for_buffer(buf, multiplier)
        })
        .collect()
}
use project::device::DeviceSettings;
use project::project::Project;
use project::block::{AudioBlockKind, InputEntry, OutputEntry};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::block::InsertBlock;
use project::chain::Chain;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceDescriptor {
    pub id: String,
    pub name: String,
    pub channels: usize,
}
#[derive(Clone)]
struct ResolvedInputDevice {
    settings: Option<DeviceSettings>,
    device: cpal::Device,
    supported: SupportedStreamConfig,
}
#[derive(Clone)]
struct ResolvedOutputDevice {
    settings: Option<DeviceSettings>,
    device: cpal::Device,
    supported: SupportedStreamConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InputStreamSignature {
    device_id: String,
    channels: Vec<usize>,
    stream_channels: u16,
    sample_rate: u32,
    buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputStreamSignature {
    device_id: String,
    channels: Vec<usize>,
    stream_channels: u16,
    sample_rate: u32,
    buffer_size_frames: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ChainStreamSignature {
    inputs: Vec<InputStreamSignature>,
    outputs: Vec<OutputStreamSignature>,
}

struct ResolvedChainAudioConfig {
    inputs: Vec<ResolvedInputDevice>,
    outputs: Vec<ResolvedOutputDevice>,
    sample_rate: f32,
    stream_signature: ChainStreamSignature,
}

struct ActiveChainRuntime {
    // Kept for diagnostics only — issue #294 removed the signature-based
    // soft-reconfig path because it silently broke audio flow. If future
    // work reintroduces a soft-reconfig fast path, this field is the
    // natural place to compare against to decide whether a rebuild is
    // needed.
    #[allow(dead_code)]
    stream_signature: ChainStreamSignature,
    _input_streams: Vec<Stream>,
    _output_streams: Vec<Stream>,
    #[cfg(all(target_os = "linux", feature = "jack"))]
    _jack_client: Option<jack::AsyncClient<JackShutdownHandler, JackProcessHandler>>,
    /// DSP worker thread handle (Linux/JACK only). Dropped when chain stops.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    _dsp_worker: Option<DspWorkerHandle>,
}

/// Handle to the DSP worker thread. Setting the stop flag and joining on drop.
#[cfg(all(target_os = "linux", feature = "jack"))]
struct DspWorkerHandle {
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    wake: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl Drop for DspWorkerHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, std::sync::atomic::Ordering::Release);
        // Wake the worker so it sees the stop flag
        if let Ok(mut flag) = self.wake.0.lock() {
            *flag = true;
        }
        self.wake.1.notify_one();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

/// Lock-free single-producer single-consumer ring buffer for passing audio
/// data from the JACK RT callback to the DSP worker thread.
///
/// The JACK callback writes interleaved f32 blocks; the worker reads them.
/// Slots are fixed-size (max_samples_per_slot), indexed by atomic counters.
#[cfg(all(target_os = "linux", feature = "jack"))]
struct SpscRingBuffer {
    /// Flat storage: `num_slots * max_samples_per_slot` f32s.
    data: Vec<std::cell::UnsafeCell<f32>>,
    /// How many f32 samples each slot holds.
    max_samples_per_slot: usize,
    /// Number of slots (power of 2 for fast modulo).
    num_slots: usize,
    /// Monotonically increasing write counter (slot index = write_pos % num_slots).
    write_pos: std::sync::atomic::AtomicUsize,
    /// Monotonically increasing read counter.
    read_pos: std::sync::atomic::AtomicUsize,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
unsafe impl Send for SpscRingBuffer {}
#[cfg(all(target_os = "linux", feature = "jack"))]
unsafe impl Sync for SpscRingBuffer {}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl SpscRingBuffer {
    fn new(num_slots: usize, max_samples_per_slot: usize) -> Self {
        assert!(num_slots.is_power_of_two());
        let total = num_slots * max_samples_per_slot;
        let mut data = Vec::with_capacity(total);
        for _ in 0..total {
            data.push(std::cell::UnsafeCell::new(0.0));
        }
        Self {
            data,
            max_samples_per_slot,
            num_slots,
            write_pos: std::sync::atomic::AtomicUsize::new(0),
            read_pos: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Try to write `samples` into the next slot. Returns false if full.
    /// SAFETY: Only one thread may call this (producer).
    fn try_write(&self, samples: &[f32]) -> bool {
        use std::sync::atomic::Ordering;
        let wp = self.write_pos.load(Ordering::Relaxed);
        let rp = self.read_pos.load(Ordering::Acquire);
        if wp.wrapping_sub(rp) >= self.num_slots {
            return false; // full
        }
        let slot = wp & (self.num_slots - 1);
        let base = slot * self.max_samples_per_slot;
        let n = samples.len().min(self.max_samples_per_slot);
        for i in 0..n {
            unsafe { *self.data[base + i].get() = samples[i]; }
        }
        // Zero remaining samples in slot
        for i in n..self.max_samples_per_slot {
            unsafe { *self.data[base + i].get() = 0.0; }
        }
        self.write_pos.store(wp.wrapping_add(1), Ordering::Release);
        true
    }

    /// Try to read the next slot into `dst`. Returns false if empty.
    /// SAFETY: Only one thread may call this (consumer).
    fn try_read(&self, dst: &mut [f32]) -> bool {
        use std::sync::atomic::Ordering;
        let rp = self.read_pos.load(Ordering::Relaxed);
        let wp = self.write_pos.load(Ordering::Acquire);
        if rp == wp {
            return false; // empty
        }
        let slot = rp & (self.num_slots - 1);
        let base = slot * self.max_samples_per_slot;
        let n = dst.len().min(self.max_samples_per_slot);
        for i in 0..n {
            dst[i] = unsafe { *self.data[base + i].get() };
        }
        self.read_pos.store(rp.wrapping_add(1), Ordering::Release);
        true
    }
}

/// JACK notification handler that survives server shutdown without calling exit().
/// When the JACK server dies (e.g. USB device unplugged), the default `()`
/// notification handler calls `std::process::exit(0)`. This handler instead
/// sets an atomic flag so the health-check timer can detect the disconnection
/// and show "Audio device disconnected" without crashing the process.
#[cfg(all(target_os = "linux", feature = "jack"))]
struct JackShutdownHandler {
    shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl jack::NotificationHandler for JackShutdownHandler {
    unsafe fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        log::warn!("JACK server shutdown: {:?} — {}", status, reason);
        self.shutdown_flag.store(true, std::sync::atomic::Ordering::Release);
        // Invalidate meta cache so is_healthy() can detect the zombie state
        // on the next health check (socket still exists but server unresponsive).
        invalidate_jack_meta_cache();
        // Do NOT call std::process::exit() — let the health timer handle it.
    }
}

/// Direct JACK process handler — runs in the JACK real-time thread.
/// Does NO DSP processing — only copies audio data to/from ring buffers.
/// The heavy DSP work happens in a separate worker thread.
///
/// Buffers are pre-allocated to avoid heap allocation in the RT callback.
#[cfg(all(target_os = "linux", feature = "jack"))]
struct JackProcessHandler {
    input_ports: Vec<jack::Port<jack::AudioIn>>,
    output_ports: Vec<jack::Port<jack::AudioOut>>,
    runtime: Arc<ChainRuntimeState>,
    input_buf: Vec<f32>,
    output_buf: Vec<f32>,
    /// Ring buffer for offloading DSP to the worker thread.
    /// When Some, the RT callback writes input to this ring and the worker
    /// thread does the processing. When None, processing is done inline
    /// (fallback for non-Linux or when worker setup fails).
    input_ring: Option<Arc<SpscRingBuffer>>,
    /// Condvar to wake the worker thread when new input is available.
    worker_wake: Option<Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>>,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl jack::ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let n_frames = ps.n_frames() as usize;

        // --- Input: read from JACK ports, interleave ---
        let total_in_ports = self.input_ports.len();
        if total_in_ports > 0 {
            let needed = n_frames * total_in_ports;
            if self.input_buf.len() < needed {
                self.input_buf.resize(needed, 0.0);
            }
            let buf = &mut self.input_buf[..needed];
            for (ch, port) in self.input_ports.iter().enumerate() {
                let port_data = port.as_slice(ps);
                for frame in 0..n_frames {
                    buf[frame * total_in_ports + ch] = port_data[frame];
                }
            }

            if let Some(ring) = &self.input_ring {
                // Offload: write to ring buffer, wake worker
                let _ = ring.try_write(buf);
                if let Some(wake) = &self.worker_wake {
                    // Non-blocking: just set flag and notify
                    if let Ok(mut flag) = wake.0.try_lock() {
                        *flag = true;
                    }
                    wake.1.notify_one();
                }
            } else {
                // Fallback: process inline (no worker thread)
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    process_input_f32(&self.runtime, 0, buf, total_in_ports);
                }));
            }
        }

        // --- Output: pull from engine, deinterleave into JACK ports ---
        // This is lightweight — just pops from ElasticBuffer, no DSP.
        let total_out_ports = self.output_ports.len();
        if total_out_ports > 0 {
            let needed = n_frames * total_out_ports;
            if self.output_buf.len() < needed {
                self.output_buf.resize(needed, 0.0);
            }
            let buf = &mut self.output_buf[..needed];
            buf.fill(0.0);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                process_output_f32(&self.runtime, 0, buf, total_out_ports);
            }));
            for (ch, port) in self.output_ports.iter_mut().enumerate() {
                let port_data = port.as_mut_slice(ps);
                for frame in 0..n_frames {
                    port_data[frame] = buf[frame * total_out_ports + ch];
                }
            }
        }

        jack::Control::Continue
    }
}

/// Pin the calling thread to the given CPU cores (Linux only).
#[cfg(all(target_os = "linux", feature = "jack"))]
fn pin_thread_to_cpus(cpus: &[usize]) {
    use std::mem;
    unsafe {
        let mut set: libc::cpu_set_t = mem::zeroed();
        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }
        let ret = libc::sched_setaffinity(0, mem::size_of::<libc::cpu_set_t>(), &set);
        if ret != 0 {
            log::warn!("sched_setaffinity failed: {}", std::io::Error::last_os_error());
        }
    }
}

/// Detect big cores on ARM big.LITTLE by reading max frequency from sysfs.
/// Returns CPU indices sorted by max frequency (highest first).
/// Falls back to CPUs 4-7 if sysfs is unavailable.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn detect_big_cores() -> Vec<usize> {
    let mut cpu_freqs: Vec<(usize, u64)> = Vec::new();
    for cpu in 0..16 {
        let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/cpuinfo_max_freq", cpu);
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Ok(freq) = contents.trim().parse::<u64>() {
                cpu_freqs.push((cpu, freq));
            }
        }
    }
    if cpu_freqs.is_empty() {
        log::info!("DSP worker: sysfs unavailable, defaulting to CPUs 4-7");
        return vec![4, 5, 6, 7];
    }
    let max_freq = cpu_freqs.iter().map(|(_, f)| *f).max().unwrap_or(0);
    let big: Vec<usize> = cpu_freqs.iter()
        .filter(|(_, f)| *f == max_freq)
        .map(|(cpu, _)| *cpu)
        .collect();
    log::info!("DSP worker: detected big cores {:?} (max_freq={}kHz)", big, max_freq);
    big
}

#[cfg(all(target_os = "linux", feature = "jack"))]
fn build_jack_direct_chain(
    chain_id: &ChainId,
    chain: &Chain,
    runtime: Arc<ChainRuntimeState>,
) -> Result<(jack::AsyncClient<JackShutdownHandler, JackProcessHandler>, DspWorkerHandle)> {
    // Determine which named JACK server this chain should connect to.
    let cards = detect_all_usb_audio_cards();
    let server_name = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .find_map(|entry| {
            if let Some(name) = entry.device_id.0.strip_prefix("jack:") {
                return Some(name.to_string());
            }
            if let Some(hw_num) = entry.device_id.0.strip_prefix("hw:") {
                if let Some(card) = cards.iter().find(|c| c.card_num == hw_num) {
                    return Some(card.server_name.clone());
                }
            }
            None
        })
        .or_else(|| {
            cards.iter()
                .find(|c| jack_server_is_running_for(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .unwrap_or_else(|| "default".to_string());

    log::info!(
        "build_jack_direct_chain: chain '{}' → JACK server '{}'",
        chain_id.0, server_name
    );

    let client_name = format!("openrig_{}", chain_id.0);
    // Retry up to 5 times with 200ms between attempts.
    // The JACK UNIX socket appears before the shm segments are fully initialized,
    // so the first connection attempt can fail with "Cannot open shm segment".
    let result = (|| {
        for attempt in 0..5u32 {
            let _lock = JACK_CONNECT_LOCK.lock().unwrap();
            std::env::set_var("JACK_DEFAULT_SERVER", &server_name);
            let r = jack::Client::new(&client_name, jack::ClientOptions::NO_START_SERVER);
            std::env::remove_var("JACK_DEFAULT_SERVER");
            drop(_lock);
            match r {
                Ok(ok) => return Ok(ok),
                Err(e) => {
                    if attempt < 4 {
                        log::warn!(
                            "JACK client '{}' connect attempt {} failed ({:?}), retrying in 200ms",
                            client_name, attempt + 1, e
                        );
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    } else {
                        return Err(e);
                    }
                }
            }
        }
        unreachable!()
    })();
    let (client, _status) = result
        .map_err(|e| anyhow!("failed to create JACK client for server '{}': {:?}", server_name, e))?;

    let sample_rate = client.sample_rate() as f32;
    let buf_size = client.buffer_size() as usize;
    log::info!(
        "JACK direct: client '{}', sample_rate={}, buffer_size={}",
        client_name, sample_rate, buf_size
    );

    // Collect input channel requirements from chain
    let input_entries: Vec<&InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter())
        .collect();

    let max_in_ch = input_entries.iter()
        .flat_map(|e| e.channels.iter())
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(1);

    // Collect output channel requirements from chain
    let output_entries: Vec<&OutputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter())
        .collect();

    let max_out_ch = output_entries.iter()
        .flat_map(|e| e.channels.iter())
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(2);

    // Register JACK ports
    let mut input_ports = Vec::new();
    for i in 0..max_in_ch {
        let port = client
            .register_port(&format!("in_{}", i + 1), jack::AudioIn::default())
            .map_err(|e| anyhow!("failed to register JACK input port {}: {:?}", i, e))?;
        input_ports.push(port);
    }

    let mut output_ports = Vec::new();
    for i in 0..max_out_ch {
        let port = client
            .register_port(&format!("out_{}", i + 1), jack::AudioOut::default())
            .map_err(|e| anyhow!("failed to register JACK output port {}: {:?}", i, e))?;
        output_ports.push(port);
    }

    // Set up DSP worker thread with ring buffer
    let samples_per_buffer = buf_size * max_in_ch;
    // 8 slots: enough headroom for JACK to write while worker processes
    let ring = Arc::new(SpscRingBuffer::new(8, samples_per_buffer));
    let wake = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let handler = JackProcessHandler {
        input_buf: vec![0.0f32; buf_size * input_ports.len().max(1)],
        output_buf: vec![0.0f32; buf_size * output_ports.len().max(1)],
        input_ports,
        output_ports,
        runtime: Arc::clone(&runtime),
        input_ring: Some(Arc::clone(&ring)),
        worker_wake: Some(Arc::clone(&wake)),
    };

    // Spawn DSP worker thread
    let worker_runtime = Arc::clone(&runtime);
    let worker_ring = Arc::clone(&ring);
    let worker_wake = Arc::clone(&wake);
    let worker_stop = Arc::clone(&stop_flag);
    let worker_channels = max_in_ch;
    let worker_chain_id = chain_id.0.clone();
    let thread = std::thread::Builder::new()
        .name(format!("dsp-worker-{}", chain_id.0))
        .spawn(move || {
            // Pin to big cores (A76 on RK3588)
            let big_cores = detect_big_cores();
            if !big_cores.is_empty() {
                pin_thread_to_cpus(&big_cores);
                log::info!("DSP worker '{}': pinned to cores {:?}", worker_chain_id, big_cores);
            }

            // Set high priority (not RT, but high normal)
            unsafe {
                let param = libc::sched_param { sched_priority: 0 };
                libc::sched_setscheduler(0, libc::SCHED_OTHER, &param);
                // Use nice -10 for higher scheduling priority
                libc::setpriority(libc::PRIO_PROCESS, 0, -10);
            }

            let mut read_buf = vec![0.0f32; samples_per_buffer];
            log::info!("DSP worker '{}': started (buf_size={}, channels={})", worker_chain_id, buf_size, worker_channels);

            loop {
                if worker_stop.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }

                // Process all available buffers
                let mut processed_any = false;
                while worker_ring.try_read(&mut read_buf) {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&worker_runtime, 0, &read_buf, worker_channels);
                    }));
                    processed_any = true;
                }

                if !processed_any {
                    // Wait for wake signal (with timeout to check stop flag)
                    let lock = worker_wake.0.lock().unwrap();
                    let _ = worker_wake.1.wait_timeout(lock, std::time::Duration::from_millis(10));
                }
            }
            log::info!("DSP worker '{}': stopped", worker_chain_id);
        })
        .map_err(|e| anyhow!("failed to spawn DSP worker thread: {}", e))?;

    let worker_handle = DspWorkerHandle {
        stop_flag,
        wake,
        thread: Some(thread),
    };

    let shutdown_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notification_handler = JackShutdownHandler { shutdown_flag };
    let active_client = client.activate_async(notification_handler, handler)
        .map_err(|e| anyhow!("failed to activate JACK client: {:?}", e))?;

    // Connect to system ports
    for i in 0..max_in_ch {
        let src = format!("system:capture_{}", i + 1);
        let dst = format!("{}:in_{}", client_name, i + 1);
        if let Err(e) = active_client.as_client().connect_ports_by_name(&src, &dst) {
            log::warn!("JACK: failed to connect {} → {}: {:?}", src, dst, e);
        }
    }
    for i in 0..max_out_ch {
        let src = format!("{}:out_{}", client_name, i + 1);
        let dst = format!("system:playback_{}", i + 1);
        if let Err(e) = active_client.as_client().connect_ports_by_name(&src, &dst) {
            log::warn!("JACK: failed to connect {} → {}: {:?}", src, dst, e);
        }
    }

    log::info!(
        "JACK direct: chain '{}' active with {} input(s), {} output(s), DSP worker on big cores",
        chain_id.0, max_in_ch, max_out_ch
    );

    Ok((active_client, worker_handle))
}

pub struct ProjectRuntimeController {
    runtime_graph: RuntimeGraph,
    active_chains: HashMap<ChainId, ActiveChainRuntime>,
}
pub fn list_devices() -> Result<Vec<String>> {
    log::trace!("listing all audio devices");

    // On Linux with the jack feature, JACK is the only supported backend for
    // audio streaming. Never fall through to CPAL/ALSA — probing a broken USB
    // audio device via ALSA can block indefinitely on certain kernels (RK3588
    // xHCI, for example). If JACK is not running, fail fast with a clear error.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if !jack_server_is_running() {
            bail!("JACK server is not running — start jackd before enumerating devices");
        }
        let inputs = jack_enumerate_input_devices()?;
        let outputs = jack_enumerate_output_devices()?;
        let mut devices = Vec::new();
        for d in inputs { devices.push(format!("input: {} | device_id: {}", d.name, d.id)); }
        for d in outputs { devices.push(format!("output: {} | device_id: {}", d.name, d.id)); }
        return Ok(devices);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        let mut devices = Vec::new();
        for device in host.input_devices()? {
            let description = device.description()?;
            devices.push(format!(
                "input: {} | device_id: {}",
                description,
                device.id()?
            ));
        }
        for device in host.output_devices()? {
            let description = device.description()?;
            devices.push(format!(
                "output: {} | device_id: {}",
                description,
                device.id()?
            ));
        }
        Ok(devices)
    }
}

/// On Linux/ALSA, cpal lists all logical devices (equivalent to `aplay -L`),
/// which includes dozens of virtual entries per card (surround51, iec958, dmix,
/// plughw, default, etc.). Only hardware devices (`hw:`) are meaningful for
/// the device picker — they map 1:1 to physical cards.
///
/// On other platforms this function always returns true (no filtering needed).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn is_hardware_device(id: &str) -> bool {
    // cpal formats device IDs as "host:pcm_id", e.g. "alsa:hw:CARD=Gen,DEV=0".
    // On Linux/ALSA, only keep hw: entries — direct hardware, one per physical
    // card/device. Skips plughw, default, surround51, iec958, dmix, etc.
    //
    // cpal enumerates each card twice: once via HintIter (named form:
    // hw:CARD=Gen,DEV=0) and once via hardware scan (numeric form:
    // hw:CARD=1,DEV=0). The two forms may have slightly different device names
    // ("USB Audio Interface, USB Audio" vs "USB Audio Interface"), defeating
    // name-based deduplication. Reject numeric CARD= forms so only the named
    // form survives — it is stable (card numbers can change on reboot).
    #[cfg(target_os = "linux")]
    {
        // When using the JACK host, device IDs start with "jack:" — always accept them.
        if id.starts_with("jack:") {
            return true;
        }
        // For ALSA, only keep hw: entries — direct hardware, one per physical
        // card/device. Skips plughw, default, surround51, iec958, dmix, etc.
        //
        // cpal enumerates each card twice: once via HintIter (named form:
        // hw:CARD=Gen,DEV=0) and once via hardware scan (numeric form:
        // hw:CARD=1,DEV=0). The two forms may have slightly different device names
        // ("USB Audio Interface, USB Audio" vs "USB Audio Interface"), defeating
        // name-based deduplication. Reject numeric CARD= forms so only the named
        // form survives — it is stable (card numbers can change on reboot).
        let pcm_id = id.split_once(':').map(|(_, d)| d).unwrap_or(id);
        if !pcm_id.starts_with("hw:") {
            return false;
        }
        // Accept only named CARD forms: hw:CARD=<letter>...
        // Reject numeric CARD forms: hw:CARD=<digit>...
        let after_card = pcm_id.split("CARD=").nth(1).unwrap_or("");
        !after_card.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = id;
        true
    }
}

// ── Device descriptor cache ─────────────────────────────────────────────────
// Device enumeration (CPAL or JACK) is expensive — on macOS CoreAudio takes
// 200-500ms, on Linux/JACK the first connection takes similar time. The UI
// calls refresh_input/output_devices on every click (30+ call sites). Cache
// the result with a TTL so a click storm produces at most one enumeration
// per window. Concurrent refreshes coalesce via try_lock — extra callers
// receive the current snapshot rather than queueing another enumeration.

const DEVICE_CACHE_TTL: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct TimedDeviceCache {
    devices: Option<Vec<AudioDeviceDescriptor>>,
    fetched_at: Option<Instant>,
}

impl TimedDeviceCache {
    const fn new() -> Self {
        Self { devices: None, fetched_at: None }
    }
    fn is_fresh(&self) -> bool {
        self.fetched_at.map(|t| t.elapsed() < DEVICE_CACHE_TTL).unwrap_or(false)
    }
}

static INPUT_DEVICE_CACHE: Mutex<TimedDeviceCache> = Mutex::new(TimedDeviceCache::new());
static OUTPUT_DEVICE_CACHE: Mutex<TimedDeviceCache> = Mutex::new(TimedDeviceCache::new());
static INPUT_REFRESH_LOCK: Mutex<()> = Mutex::new(());
static OUTPUT_REFRESH_LOCK: Mutex<()> = Mutex::new(());

/// Force-stale the device cache so the next list_*_device_descriptors() call
/// re-enumerates even if the TTL has not elapsed. Call this when we know the
/// topology changed (hot-plug detected).
pub fn invalidate_device_cache() {
    *INPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache::new();
    *OUTPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache::new();
    #[cfg(all(target_os = "linux", feature = "jack"))]
    invalidate_proc_cache();
    log::info!("device descriptor cache invalidated");
}

// ── Hotplug detection ────────────────────────────────────────────────────────
// Cheap device count used by the health timer to detect plug-in events without
// running a full enumeration (no ALSA PCM probe, no JACK client connection).

static LAST_KNOWN_DEVICE_COUNT: Mutex<Option<usize>> = Mutex::new(None);

/// Returns `true` when the audio device count has increased since the last
/// call, indicating that a new interface was plugged in.
///
/// Intentionally cheap — no ALSA probing, no JACK connection. Call from a
/// periodic UI timer; on `true` follow up with `invalidate_device_cache()` and
/// a full device-list refresh.
pub fn has_new_devices() -> bool {
    let current = count_devices_cheap();
    let mut guard = LAST_KNOWN_DEVICE_COUNT.lock().unwrap();
    match *guard {
        None => {
            *guard = Some(current);
            false
        }
        Some(prev) if current > prev => {
            *guard = Some(current);
            log::info!("has_new_devices: count {} → {}", prev, current);
            true
        }
        Some(prev) => {
            if current != prev {
                *guard = Some(current);
            }
            false
        }
    }
}

/// Count audio devices cheaply — no ALSA PCM probing, no JACK client.
fn count_devices_cheap() -> usize {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        // Pure /proc/asound/cards read — safe, no PCM open, no JACK connection.
        return detect_all_usb_audio_cards().len();
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = select_host_for_enumeration();
        let input = host.input_devices().map(|it| it.count()).unwrap_or(0);
        let output = host.output_devices().map(|it| it.count()).unwrap_or(0);
        input + output
    }
}

/// Returns true if the JACK server is currently running.
/// Fast, non-blocking check — safe to call from the UI thread.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub fn jack_is_running() -> bool {
    jack_server_is_running()
}

/// Start JACK in background threads — one per connected USB audio interface.
/// Returns a channel that resolves when ALL servers are ready (Ok) or any fails (Err).
/// Non-blocking — returns immediately. Poll the receiver from a UI timer.
#[cfg(all(target_os = "linux", feature = "jack"))]
pub fn start_jack_in_background(
    device_settings: Vec<DeviceSettings>,
) -> std::sync::mpsc::Receiver<anyhow::Result<()>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            let cards = detect_all_usb_audio_cards();
            if cards.is_empty() {
                anyhow::bail!(
                    "no USB audio interface found — connect a device before enabling a chain"
                );
            }
            for card in &cards {
                if jack_server_is_running_for(&card.server_name) {
                    log::info!(
                        "start_jack_in_background: server '{}' already running",
                        card.server_name
                    );
                    continue;
                }
                // Sample rate and buffer size are per-device, not per-project.
                // Look up the settings for this specific card by device_id; fall
                // back to sensible defaults if the user hasn't configured this
                // device yet.
                let (sample_rate, buffer_size) = device_settings
                    .iter()
                    .find(|s| s.device_id.0 == card.device_id)
                    .map(|s| (s.sample_rate, s.buffer_size_frames))
                    .unwrap_or((48000, 64));
                // Retry up to 3 times. The first attempt sometimes fails with
                // "Broken pipe" when the ALSA device needs a moment to settle
                // after a previous jackd exit or USB reconnect.
                let mut last_err = anyhow::anyhow!("no attempt made");
                for attempt in 1u32..=3 {
                    match launch_jackd(card, sample_rate, buffer_size) {
                        Ok(()) => { last_err = anyhow::anyhow!("ok"); break; }
                        Err(e) => {
                            log::warn!(
                                "start_jack_in_background: attempt {}/3 for '{}' failed: {}",
                                attempt, card.server_name, e
                            );
                            last_err = e;
                            if attempt < 3 {
                                // Kill only this server's orphan jackd — never
                                // killall, since other interfaces/chains may be
                                // running their own healthy jackd.
                                let _ = stop_jackd_for(&card.server_name);
                                std::thread::sleep(std::time::Duration::from_secs(1));
                            }
                        }
                    }
                }
                if !jack_server_is_running_for(&card.server_name) {
                    return Err(last_err);
                }
            }
            invalidate_device_cache();
            Ok(())
        })();
        let _ = tx.send(result);
    });
    rx
}

/// Apply device settings (sample rate, buffer size) to hardware devices
/// without requiring active chains. On macOS/CoreAudio, building a stream
/// with the desired sample rate forces the driver to reconfigure the device.
/// The temporary stream is dropped immediately after configuration.
///
/// USB audio devices may take a few seconds to reconfigure.
/// cpal may report a timeout even though the change succeeds — we treat
/// timeouts as warnings and wait for the device to settle.
pub fn apply_device_settings(settings: &[DeviceSettings]) -> Result<()> {
    if settings.is_empty() {
        return Ok(());
    }
    // On Linux with JACK, jackd is always launched with the correct sample_rate
    // and buffer_size from gui-settings.yaml via ensure_jack_running(). Never
    // probe the ALSA PCM here — on RK3588 xHCI, calling supported_input_configs()
    // on Linux/JACK, probing the ALSA PCM can disturb USB audio devices.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        log::info!("apply_device_settings: Linux/JACK — skipping ALSA probe (jackd owns device config)");
        return Ok(());
    }
    // macOS / Windows path: build a temporary stream to force the CoreAudio /
    // WASAPI driver to adopt the requested sample rate and buffer size.
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        let mut needs_settle = false;
        for ds in settings {
            log::info!(
                "apply_device_settings: configuring '{}' sr={} buf={}",
                ds.device_id.0, ds.sample_rate, ds.buffer_size_frames
            );
            // Try as input device first — on macOS the same physical device
            // often shares one AudioObjectID for both directions, so configuring
            // the input side sets the sample rate for the whole device.
            if let Ok(Some(device)) = find_input_device_by_id(host, &ds.device_id.0) {
                // Check if device already at requested sample rate
                let current_rate = device.default_input_config()
                    .map(|c| c.sample_rate())
                    .unwrap_or(0);
                if current_rate == ds.sample_rate {
                    log::info!(
                        "apply_device_settings: input '{}' already at sr={}, skipping",
                        ds.device_id.0, ds.sample_rate
                    );
                    continue;
                }
                if let Ok(ranges) = device.supported_input_configs() {
                    let ranges: Vec<_> = ranges.collect();
                    if let Some(config) = ranges.iter()
                        .filter(|r| r.channels() >= 1)
                        .filter_map(|r| r.try_with_sample_rate(ds.sample_rate))
                        .next()
                    {
                        let stream_config = build_stream_config(
                            config.channels(),
                            ds.sample_rate,
                            ds.buffer_size_frames,
                        );
                        match device.build_input_stream(
                            &stream_config,
                            |_data: &[f32], _| {},
                            |err| log::warn!("apply_device_settings input error: {err}"),
                            None,
                        ) {
                            Ok(stream) => {
                                log::info!(
                                    "apply_device_settings: input device '{}' configured (sr={} buf={})",
                                    ds.device_id.0, ds.sample_rate, ds.buffer_size_frames
                                );
                                drop(stream);
                            }
                            Err(e) => {
                                // USB audio devices may timeout during sample rate
                                // change but still reconfigure successfully. Treat as warning.
                                let msg = e.to_string();
                                if msg.contains("timeout") {
                                    log::info!(
                                        "apply_device_settings: device '{}' sample rate change in progress (timeout is normal for USB devices)",
                                        ds.device_id.0
                                    );
                                    needs_settle = true;
                                } else {
                                    log::warn!(
                                        "apply_device_settings: failed to configure input '{}': {e}",
                                        ds.device_id.0
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if needs_settle {
            log::info!("apply_device_settings: waiting 3s for USB device to settle after sample rate change");
            std::thread::sleep(std::time::Duration::from_secs(3));
            // Invalidate device cache since supported configs may have changed
            invalidate_device_cache();
        }
        return Ok(());
    }
}

pub fn list_input_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    // Fast path: TTL still fresh, return the cached copy.
    let fresh = {
        let cache = INPUT_DEVICE_CACHE.lock().unwrap();
        if cache.is_fresh() {
            cache.devices.clone()
        } else {
            None
        }
    };
    if let Some(devices) = fresh {
        log::trace!("list_input_device_descriptors: cache hit ({} devices)", devices.len());
        return Ok(devices);
    }
    // Slow path: try to acquire the refresh lock. If another thread is
    // already refreshing, return whatever stale copy we have instead of
    // running a concurrent enumeration.
    match INPUT_REFRESH_LOCK.try_lock() {
        Ok(_guard) => {
            // Double-check in case a concurrent refresh just finished.
            let already_fresh = {
                let cache = INPUT_DEVICE_CACHE.lock().unwrap();
                if cache.is_fresh() { cache.devices.clone() } else { None }
            };
            if let Some(devices) = already_fresh {
                return Ok(devices);
            }
            log::info!("list_input_device_descriptors: cache stale, enumerating...");
            let devices = enumerate_input_devices_uncached()?;
            *INPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
                devices: Some(devices.clone()),
                fetched_at: Some(Instant::now()),
            };
            Ok(devices)
        }
        Err(_) => {
            log::debug!("list_input_device_descriptors: refresh in progress, returning stale snapshot");
            Ok(INPUT_DEVICE_CACHE.lock().unwrap().devices.clone().unwrap_or_default())
        }
    }
}

pub fn list_output_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    let fresh = {
        let cache = OUTPUT_DEVICE_CACHE.lock().unwrap();
        if cache.is_fresh() {
            cache.devices.clone()
        } else {
            None
        }
    };
    if let Some(devices) = fresh {
        log::trace!("list_output_device_descriptors: cache hit ({} devices)", devices.len());
        return Ok(devices);
    }
    match OUTPUT_REFRESH_LOCK.try_lock() {
        Ok(_guard) => {
            let already_fresh = {
                let cache = OUTPUT_DEVICE_CACHE.lock().unwrap();
                if cache.is_fresh() { cache.devices.clone() } else { None }
            };
            if let Some(devices) = already_fresh {
                return Ok(devices);
            }
            log::info!("list_output_device_descriptors: cache stale, enumerating...");
            let devices = enumerate_output_devices_uncached()?;
            *OUTPUT_DEVICE_CACHE.lock().unwrap() = TimedDeviceCache {
                devices: Some(devices.clone()),
                fetched_at: Some(Instant::now()),
            };
            Ok(devices)
        }
        Err(_) => {
            log::debug!("list_output_device_descriptors: refresh in progress, returning stale snapshot");
            Ok(OUTPUT_DEVICE_CACHE.lock().unwrap().devices.clone().unwrap_or_default())
        }
    }
}

fn enumerate_input_devices_uncached() -> Result<Vec<AudioDeviceDescriptor>> {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            return jack_enumerate_input_devices();
        }
        // JACK not running — detect USB audio cards from /proc/asound/cards and
        // return them with jack:<name> device IDs, matching what is stored in
        // project YAML. This avoids calling supported_input_configs() (which
        // opens the PCM directly) and ensures device_id
        // consistency regardless of ALSA card numbering order (hw:0 vs hw:1).
        invalidate_jack_meta_cache();
        log::info!("JACK not running, detecting USB audio cards for input devices (no PCM probe)");
        let usb_cards = detect_all_usb_audio_cards();
        let cards: Vec<AudioDeviceDescriptor> = usb_cards.iter().map(|c| AudioDeviceDescriptor {
            id: c.device_id.clone(),
            name: c.display_name.clone(),
            channels: c.capture_channels as usize,
        }).collect();
        log::info!("[enumerate_input] usb cards: {} devices", cards.len());
        return Ok(cards);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = select_host_for_enumeration();
        let mut devices = Vec::new();
        for device in host.input_devices()? {
            let id = device.id()?.to_string();
            if !is_hardware_device(&id) {
                continue;
            }
            let name = device.description()?.name().to_string();
            if devices.iter().any(|d: &AudioDeviceDescriptor| d.name == name) {
                continue;
            }
            let ch = max_supported_input_channels(&device).unwrap_or(0);
            log::info!("[enumerate_input] device id='{}' name='{}' channels={}", id, name, ch);
            devices.push(AudioDeviceDescriptor { id, name, channels: ch });
        }
        log::info!("[enumerate_input] total {} devices", devices.len());
        devices.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(devices)
    }
}

fn enumerate_output_devices_uncached() -> Result<Vec<AudioDeviceDescriptor>> {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            return jack_enumerate_output_devices();
        }
        invalidate_jack_meta_cache();
        log::info!("JACK not running, detecting USB audio cards for output devices (no PCM probe)");
        let usb_cards = detect_all_usb_audio_cards();
        let cards: Vec<AudioDeviceDescriptor> = usb_cards.iter().map(|c| AudioDeviceDescriptor {
            id: c.device_id.clone(),
            name: c.display_name.clone(),
            channels: c.playback_channels as usize,
        }).collect();
        log::info!("[enumerate_output] usb cards: {} devices", cards.len());
        return Ok(cards);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = select_host_for_enumeration();
        let mut devices = Vec::new();
        for device in host.output_devices()? {
            let id = device.id()?.to_string();
            if !is_hardware_device(&id) {
                continue;
            }
            let name = device.description()?.name().to_string();
            if devices.iter().any(|d: &AudioDeviceDescriptor| d.name == name) {
                continue;
            }
            let ch = max_supported_output_channels(&device).unwrap_or(0);
            log::info!("[enumerate_output] device id='{}' name='{}' channels={}", id, name, ch);
            devices.push(AudioDeviceDescriptor { id, name, channels: ch });
        }
        log::info!("[enumerate_output] total {} devices", devices.len());
        devices.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(devices)
    }
}
pub fn build_streams_for_project(
    project: &Project,
    runtime_graph: &RuntimeGraph,
) -> Result<Vec<Stream>> {
    log::info!("building audio streams for project");

    // On Linux with JACK, no CPAL streams are ever needed — streaming is handled
    // entirely by the jack crate in build_active_chain_runtime. Also, calling
    // validate_channels_against_devices() here would probe ALSA PCM and disturb
    // USB audio devices.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = project;       // not needed on Linux/JACK
        let _ = runtime_graph; // not needed on Linux/JACK: all streaming handled by jack crate
        return Ok(Vec::new());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        validate_channels_against_devices(project, host)?;
        let mut resolved_chains = resolve_enabled_chain_audio_configs(host, project)?;
        let mut streams = Vec::new();
        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let runtime = runtime_graph
                .chains
                .get(&chain.id)
                .cloned()
                .ok_or_else(|| anyhow!("chain '{}' has no runtime state", chain.id.0))?;
            let resolved = resolved_chains
                .remove(&chain.id)
                .ok_or_else(|| anyhow!("chain '{}' missing resolved audio config", chain.id.0))?;
            let (input_streams, output_streams) = build_chain_streams(&chain.id, resolved, runtime)?;
            streams.extend(input_streams);
            streams.extend(output_streams);
        }
        Ok(streams)
    }
}

/// Build a synthetic ResolvedChainAudioConfig using only the jack crate.
/// No CPAL or ALSA access. The resolved config is only used to provide
/// sample_rate and stream_signature to the runtime graph — the direct JACK
/// backend ignores inputs/outputs entirely.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_resolve_chain_config(chain: &Chain) -> Result<ResolvedChainAudioConfig> {
    // Resolve the JACK server for this chain by inspecting its I/O device_ids.
    // Chain entries may have:
    //   - "jack:<server_name>"  → use that server directly
    //   - "hw:<N>"              → find the card at hw:N and use its server
    //   - anything else         → fall back to first running server
    let cards = detect_all_usb_audio_cards();

    let resolve_server = |device_id: &str| -> Option<String> {
        if let Some(name) = device_id.strip_prefix("jack:") {
            // Explicit jack:<name>
            return Some(name.to_string());
        }
        if let Some(hw_num) = device_id.strip_prefix("hw:") {
            // hw:N → find matching card by card_num
            if let Some(card) = cards.iter().find(|c| c.card_num == hw_num) {
                return Some(card.server_name.clone());
            }
        }
        // Fallback: first running server
        cards.iter()
            .find(|c| jack_server_is_running_for(&c.server_name))
            .map(|c| c.server_name.clone())
    };

    // Determine server from first input entry, or fallback to first running
    let server_name = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .find_map(|entry| resolve_server(&entry.device_id.0))
        .or_else(|| {
            cards.iter()
                .find(|c| jack_server_is_running_for(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .ok_or_else(|| anyhow!("no running JACK server found for chain"))?;

    let meta = jack_meta_for(&server_name)?;
    let device_id = format!("jack:{}", server_name);
    let sample_rate = meta.sample_rate as f32;
    let in_channels = meta.capture_port_count as u16;
    let out_channels = meta.playback_port_count as u16;

    let input_sigs: Vec<InputStreamSignature> = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .map(|entry| InputStreamSignature {
            device_id: device_id.clone(),
            channels: entry.channels.clone(),
            stream_channels: in_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    let output_sigs: Vec<OutputStreamSignature> = chain.output_blocks().into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .map(|entry| OutputStreamSignature {
            device_id: device_id.clone(),
            channels: entry.channels.clone(),
            stream_channels: out_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    Ok(ResolvedChainAudioConfig {
        inputs: Vec::new(),
        outputs: Vec::new(),
        sample_rate,
        stream_signature: ChainStreamSignature {
            inputs: input_sigs,
            outputs: output_sigs,
        },
    })
}

impl ProjectRuntimeController {
    pub fn start(project: &Project) -> Result<Self> {
        log::info!("starting project runtime controller");
        let mut controller = Self {
            runtime_graph: RuntimeGraph {
                chains: HashMap::new(),
            },
            active_chains: HashMap::new(),
        };
        controller.sync_project(project)?;
        Ok(controller)
    }

    pub fn sync_project(&mut self, project: &Project) -> Result<()> {
        log::debug!("syncing project runtime with {} chains", project.chains.len());

        // On Linux with JACK feature, only start jackd when the project has
        // at least one enabled chain that actually needs audio. Launching
        // jackd opens the ALSA PCM for each card, which exercises the USB
        // audio stack — we must not do that passively while the user is just
        // editing chain settings with everything bypassed.
        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            let needs_audio = project.chains.iter().any(|c| c.enabled);
            if !needs_audio {
                log::debug!(
                    "sync_project: no enabled chains, skipping ensure_jack_running"
                );
                if !self.active_chains.is_empty() {
                    log::info!("sync_project: no enabled chains, tearing down runtime");
                    self.stop();
                }
                return Ok(());
            }
            // Tear down active chains BEFORE ensure_jack_running kills the
            // current jackd. If we drop the AsyncClients after the server
            // died, jack-rs global state ends up confused and new client
            // connections fail with FAILURE|SERVER_ERROR even though the
            // fresh jackd is alive and accepting external clients. See
            // jack_restart_would_be_triggered() doc-comment.
            if !self.active_chains.is_empty() && jack_restart_would_be_triggered(project) {
                log::info!("sync_project: JACK restart imminent, tearing down chains first");
                self.stop();
            }
            let jack_restarted = ensure_jack_running(project)?;
            if jack_restarted && !self.active_chains.is_empty() {
                // Should never happen now that the teardown above runs first,
                // but kept as a safety net: if a card was added mid-run and a
                // fresh launch happened without the predicate catching it,
                // drop anything still around before the upsert re-registers.
                log::info!("sync_project: JACK restarted with active chains still present, tearing down");
                self.stop();
            }
            return self.sync_project_jack_direct(project);
        }

        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        {
            let host = get_host();
            validate_channels_against_devices(project, host)?;
            let mut resolved_chains = resolve_enabled_chain_audio_configs(host, project)?;

            let removed_chain_ids = self
                .active_chains
                .keys()
                .filter(|chain_id| !resolved_chains.contains_key(*chain_id))
                .cloned()
                .collect::<Vec<_>>();
            for chain_id in removed_chain_ids {
                log::info!("removing chain '{}' from runtime", chain_id.0);
                if let Some(runtime) = self.runtime_graph.runtime_for_chain(&chain_id) {
                    runtime.set_draining();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                self.active_chains.remove(&chain_id);
                self.runtime_graph.remove_chain(&chain_id);
            }

            for chain in &project.chains {
                if !chain.enabled {
                    continue;
                }

                let resolved = resolved_chains
                    .remove(&chain.id)
                    .ok_or_else(|| anyhow!("chain '{}' missing resolved audio config", chain.id.0))?;
                self.upsert_chain_with_resolved(chain, resolved)?;
            }

            Ok(())
        }
    }

    /// Sync project using only the jack crate — zero CPAL/ALSA access.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    fn sync_project_jack_direct(&mut self, project: &Project) -> Result<()> {
        log::info!("sync_project: JACK direct mode (no CPAL/ALSA)");

        // Remove chains that are no longer in the project
        let active_ids: Vec<ChainId> = self.active_chains.keys().cloned().collect();
        for chain_id in active_ids {
            let still_exists = project.chains.iter().any(|c| c.enabled && c.id == chain_id);
            if !still_exists {
                log::info!("removing chain '{}' from runtime", chain_id.0);
                // Signal the audio callback to stop processing blocks BEFORE
                // deactivating the JACK client — prevents use-after-free in C++
                // NAM destructors ("terminate called without active exception").
                if let Some(runtime) = self.runtime_graph.runtime_for_chain(&chain_id) {
                    runtime.set_draining();
                    // Give the JACK callback time to finish its current cycle.
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                self.active_chains.remove(&chain_id);
                self.runtime_graph.remove_chain(&chain_id);
            }
        }

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let resolved = jack_resolve_chain_config(chain)?;
            self.upsert_chain_with_resolved(chain, resolved)?;
        }

        Ok(())
    }

    pub fn upsert_chain(&mut self, project: &Project, chain: &Chain) -> Result<()> {
        log::info!("upserting chain '{}', enabled={}", chain.id.0, chain.enabled);
        if !chain.enabled {
            self.remove_chain(&chain.id);
            return Ok(());
        }

        #[cfg(all(target_os = "linux", feature = "jack"))]
        {
            // Same ordering as sync_project: drop AsyncClients BEFORE the
            // jackd they reference gets killed inside ensure_jack_running.
            if !self.active_chains.is_empty() && jack_restart_would_be_triggered(project) {
                log::info!("upsert_chain: JACK restart imminent, tearing down chains first");
                self.stop();
            }
            let jack_restarted = ensure_jack_running(project)?;
            if jack_restarted && !self.active_chains.is_empty() {
                log::info!("upsert_chain: JACK restarted with active chains still present, tearing down");
                self.stop();
            }
            let resolved = jack_resolve_chain_config(chain)?;
            return self.upsert_chain_with_resolved(chain, resolved);
        }

        #[cfg(not(all(target_os = "linux", feature = "jack")))]
        {
            let host = get_host();
            validate_chain_channels_against_devices(host, chain)?;
            let resolved = resolve_chain_audio_config(host, project, chain)?;
            self.upsert_chain_with_resolved(chain, resolved)
        }
    }

    pub fn remove_chain(&mut self, chain_id: &ChainId) {
        log::info!("removing chain '{}' from runtime", chain_id.0);
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            runtime.set_draining();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.active_chains.remove(chain_id);
        self.runtime_graph.remove_chain(chain_id);
    }

    pub fn stop(&mut self) {
        log::info!("stopping project runtime controller");
        self.active_chains.clear();
        self.runtime_graph.chains.clear();
    }

    pub fn is_running(&self) -> bool {
        !self.active_chains.is_empty()
    }

    /// Check whether the audio backend is still healthy.
    ///
    /// On Linux/JACK: returns false when the JACK server has disappeared (e.g.
    /// USB audio device unplugged → udev restarts jackd). The caller should
    /// tear down the runtime and attempt reconnection once JACK reappears.
    ///
    /// On macOS/Windows (CoreAudio/WASAPI): always returns true — device loss
    /// is detected through stream error callbacks, not polling.
    pub fn is_healthy(&self) -> bool {
        if self.active_chains.is_empty() {
            return true;
        }
        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() {
            // Socket existence alone is insufficient — jackd can enter a zombie state
            // where the process is alive (socket present) but the ALSA driver has
            // stopped (e.g. after USB device disconnect). Verify the server is
            // actually responsive by checking the meta cache, which is invalidated
            // by the JackShutdownHandler when the connection drops.
            if !jack_server_is_running() {
                return false;
            }
            // If the cache is empty (was invalidated after a disconnect) and we
            // can't reach the server, report unhealthy so the health timer triggers
            // try_reconnect → ensure_jack_running → zombie kill + restart.
            let cards = detect_all_usb_audio_cards();
            for card in &cards {
                if jack_server_is_running_for(&card.server_name) {
                    if jack_meta_cache().lock().unwrap().get(&card.server_name).is_none() {
                        // Cache was cleared — server may be zombie. Probe it.
                        if jack_meta_for(&card.server_name).is_err() {
                            log::warn!("is_healthy: server '{}' unresponsive (zombie), reporting unhealthy", card.server_name);
                            return false;
                        }
                    }
                }
            }
            return true;
        }
        true
    }

    /// Attempt to reconnect after the audio backend became unhealthy.
    ///
    /// Tears down all active chains, invalidates caches, and re-syncs the
    /// project. Returns Ok(true) if reconnection succeeded, Ok(false) if the
    /// backend is not yet available (no USB device, JACK not ready).
    pub fn try_reconnect(&mut self, project: &Project) -> Result<bool> {
        log::info!("try_reconnect: checking if audio backend is available");

        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() {
            // If no USB audio card is present, don't even try
            if detect_all_usb_audio_cards().is_empty() {
                log::debug!("try_reconnect: no USB audio card found");
                return Ok(false);
            }
            invalidate_jack_meta_cache();
        }

        // Tear down everything cleanly
        self.stop();

        // Re-sync from scratch — sync_project will call ensure_jack_running
        // on Linux, which launches jackd if needed
        match self.sync_project(project) {
            Ok(()) => {
                log::info!("try_reconnect: successfully reconnected with {} chains", self.active_chains.len());
                Ok(true)
            }
            Err(e) => {
                log::warn!("try_reconnect: sync_project failed: {}", e);
                Err(e)
            }
        }
    }

    /// Returns stream data for a block in any running chain.
    pub fn poll_stream(&self, block_id: &domain::ids::BlockId) -> Option<Vec<block_core::StreamEntry>> {
        for (_, runtime) in &self.runtime_graph.chains {
            if let Some(entries) = runtime.poll_stream(block_id) {
                return Some(entries);
            }
        }
        None
    }

    /// Drains and returns all block errors that occurred since the last call.
    pub fn poll_errors(&self) -> Vec<engine::runtime::BlockError> {
        self.runtime_graph.chains.values()
            .flat_map(|runtime| runtime.poll_errors())
            .collect()
    }

    /// Returns the measured real-time latency in milliseconds for a given chain.
    pub fn measured_latency_ms(&self, chain_id: &ChainId) -> Option<f32> {
        self.runtime_graph.chains.get(chain_id)
            .map(|runtime| runtime.measured_latency_ms())
    }

    /// Arms a latency probe on the given chain: the next input callback
    /// injects a short beep, and the first output callback that sees it
    /// updates `measured_latency_ms`. No-op if the chain has no runtime.
    pub fn arm_latency_probe(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.chains.get(chain_id) {
            runtime.arm_latency_probe();
        }
    }

    /// Cancels any in-flight latency probe on the given chain. The UI
    /// calls this when the on-screen probe display window expires so a
    /// probe that never produced a detection does not stay armed.
    pub fn cancel_latency_probe(&self, chain_id: &ChainId) {
        if let Some(runtime) = self.runtime_graph.chains.get(chain_id) {
            runtime.cancel_latency_probe();
        }
    }

    fn upsert_chain_with_resolved(
        &mut self,
        chain: &Chain,
        resolved: ResolvedChainAudioConfig,
    ) -> Result<()> {
        // Rebuild the JACK client + DSP worker only when the I/O layout
        // actually changed (input/output channels, mode, sample rate, etc).
        // A block toggle / param edit keeps the same stream_signature and
        // goes through the soft-reconfig path so we don't drop audio every
        // time the user tweaks a knob. A channel (un)check flips the
        // signature and triggers teardown+rebuild (issue #294 original).
        //
        // Known caveat: some edits that DO preserve the signature have been
        // observed to leave the in-place block pipeline reading silence on
        // Linux/JACK. The workaround is toggling the chain off+on — if you
        // hit that, widen this predicate for the specific edit that broke
        // flow, don't flip the whole thing back to unconditional rebuild
        // (that regresses block toggles on RT kernels).
        let needs_stream_rebuild = self
            .active_chains
            .get(&chain.id)
            .map(|active| active.stream_signature != resolved.stream_signature)
            .unwrap_or(true);

        // Tear down the previous ActiveChainRuntime BEFORE mutating shared
        // runtime state or building the replacement. Otherwise HashMap::insert
        // drops the old runtime only after the new one is fully constructed,
        // which on JACK leaves the old client alive while the new one tries
        // to register with the same name — the new client gets a suffixed
        // name, connect_ports_by_name binds to the old client's ports, and
        // when the old runtime is finally dropped the new client is orphaned.
        if needs_stream_rebuild {
            self.teardown_active_chain_for_rebuild(&chain.id);
        }

        let elastic_targets = compute_elastic_targets_for_chain(chain, &resolved);
        let runtime = self.runtime_graph.upsert_chain(
            chain,
            resolved.sample_rate,
            needs_stream_rebuild,
            &elastic_targets,
        )?;

        if needs_stream_rebuild {
            let active = build_active_chain_runtime(&chain.id, chain, resolved, runtime)?;
            self.active_chains.insert(chain.id.clone(), active);
        }

        Ok(())
    }

    /// Drop the ActiveChainRuntime for `chain_id` so its JACK client / DSP
    /// worker / CPAL streams release their resources before a replacement is
    /// built. Drains the audio callback first (same dance as `remove_chain`)
    /// so NAM C++ destructors don't fire mid-callback.
    ///
    /// No-op when no runtime is active for that chain. Leaves
    /// `runtime_graph` untouched — the caller is about to re-upsert it.
    fn teardown_active_chain_for_rebuild(&mut self, chain_id: &ChainId) {
        if !self.active_chains.contains_key(chain_id) {
            return;
        }
        if let Some(runtime) = self.runtime_graph.runtime_for_chain(chain_id) {
            runtime.set_draining();
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.active_chains.remove(chain_id);
    }
}

pub fn resolve_project_chain_sample_rates(project: &Project) -> Result<HashMap<ChainId, f32>> {
    // On Linux+JACK, get sample rate from JACK server directly — zero ALSA access.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let sr = jack_meta()?.sample_rate as f32;
        let mut sample_rates = HashMap::new();
        for chain in &project.chains {
            if chain.enabled {
                sample_rates.insert(chain.id.clone(), sr);
            }
        }
        return Ok(sample_rates);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        let mut sample_rates = HashMap::new();

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let inputs = resolve_chain_inputs(&host, project, chain)?;
            let outputs = resolve_chain_outputs(&host, project, chain)?;
            let sample_rate = resolve_multi_io_sample_rate(&chain.id.0, &inputs, &outputs)?;
            sample_rates.insert(chain.id.clone(), sample_rate);
        }

        Ok(sample_rates)
    }
}


#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_input_device_for_chain_input(
    host: &cpal::Host,
    project: &Project,
    input: &InputEntry,
    is_asio: bool,
) -> Result<ResolvedInputDevice> {
    let settings = project
        .device_settings
        .iter()
        .find(|s| s.device_id == input.device_id)
        .cloned();
    if using_jack_direct() {
        // Unreachable in JACK-direct mode: sync_project / upsert_chain short-circuit
        // into sync_project_jack_direct() before ever calling this function. If we
        // ever land here while JACK is active, something bypassed the short-circuit
        // and is about to probe ALSA on a device JACK owns — refuse instead.
        bail!("internal error: resolve_input_device_for_chain_input called in JACK-direct mode");
    }
    let device = find_input_device_by_id(host, &input.device_id.0)?.ok_or_else(|| {
        anyhow!("input device '{}' not found by device_id", input.device_id.0)
    })?;
    let default_config = device.default_input_config().with_context(|| {
        format!(
            "failed to get default input config for '{}'",
            input.device_id.0
        )
    })?;
    let supported_ranges = device
        .supported_input_configs()
        .with_context(|| {
            format!(
                "failed to enumerate input configs for '{}'",
                input.device_id.0
            )
        })?
        .collect::<Vec<_>>();
    let required_channels = required_channel_count(&input.channels);
    let supported = select_supported_stream_config(
        &default_config,
        &supported_ranges,
        settings.as_ref().map(|s| s.sample_rate),
        required_channels,
        &input.device_id.0,
    )?;
    // For ASIO, skip buffer size range validation — the project's requested buffer size
    // is passed directly to the ASIO driver via BufferSize::Fixed. The driver accepts or
    // rejects it at stream build time with a real error. Pre-validation is incorrect for
    // ASIO because the driver's reported range reflects its current preferred size, not
    // what it actually accepts when asked.
    if !is_asio {
        if let Some(settings) = &settings {
            validate_buffer_size(
                settings.buffer_size_frames,
                supported.buffer_size(),
                &settings.device_id.0,
            )?;
        }
    }
    Ok(ResolvedInputDevice { settings, device, supported })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_output_device_for_chain_output(
    host: &cpal::Host,
    project: &Project,
    output: &OutputEntry,
    is_asio: bool,
) -> Result<ResolvedOutputDevice> {
    let settings = project
        .device_settings
        .iter()
        .find(|s| s.device_id == output.device_id)
        .cloned();
    if using_jack_direct() {
        // Unreachable in JACK-direct mode (see matching guard in the input path).
        bail!("internal error: resolve_output_device_for_chain_output called in JACK-direct mode");
    }
    let device = find_output_device_by_id(host, &output.device_id.0)?.ok_or_else(|| {
        anyhow!("output device '{}' not found by device_id", output.device_id.0)
    })?;
    let default_config = device.default_output_config().with_context(|| {
        format!(
            "failed to get default output config for '{}'",
            output.device_id.0
        )
    })?;
    let supported_ranges = device
        .supported_output_configs()
        .with_context(|| {
            format!(
                "failed to enumerate output configs for '{}'",
                output.device_id.0
            )
        })?
        .collect::<Vec<_>>();
    let required_channels = required_channel_count(&output.channels);
    let supported = select_supported_stream_config(
        &default_config,
        &supported_ranges,
        settings.as_ref().map(|s| s.sample_rate),
        required_channels,
        &output.device_id.0,
    )?;
    if !is_asio {
        if let Some(settings) = &settings {
            validate_buffer_size(
                settings.buffer_size_frames,
                supported.buffer_size(),
                &settings.device_id.0,
            )?;
        }
    }
    Ok(ResolvedOutputDevice { settings, device, supported })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_chain_inputs(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
) -> Result<Vec<ResolvedInputDevice>> {
    let is_asio = is_asio_host(host);
    let mut input_entries: Vec<&InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter())
        .collect();
    // Include Insert block return endpoints as input streams
    let insert_return_entries: Vec<InputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_return_as_input_entry(ib)),
            _ => None,
        })
        .collect();
    let insert_refs: Vec<&InputEntry> = insert_return_entries.iter().collect();
    input_entries.extend(insert_refs);
    if input_entries.is_empty() {
        bail!("chain '{}' has no input blocks configured", chain.id.0);
    }
    input_entries
        .iter()
        .map(|input| resolve_input_device_for_chain_input(host, project, input, is_asio))
        .collect()
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_chain_outputs(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
) -> Result<Vec<ResolvedOutputDevice>> {
    let is_asio = is_asio_host(host);
    let mut output_entries: Vec<&OutputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter())
        .collect();
    // Include Insert block send endpoints as output streams
    let insert_send_entries: Vec<OutputEntry> = chain.blocks.iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_send_as_output_entry(ib)),
            _ => None,
        })
        .collect();
    let insert_refs: Vec<&OutputEntry> = insert_send_entries.iter().collect();
    output_entries.extend(insert_refs);
    if output_entries.is_empty() {
        bail!("chain '{}' has no output blocks configured", chain.id.0);
    }
    output_entries
        .iter()
        .map(|output| resolve_output_device_for_chain_output(host, project, output, is_asio))
        .collect()
}

/// Convert an InsertBlock's return endpoint to an InputEntry for stream resolution.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an InsertBlock's send endpoint to an OutputEntry for stream resolution.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
    use project::chain::ChainOutputMode;
    OutputEntry {
        device_id: insert.send.device_id.clone(),
        mode: match insert.send.mode {
            project::chain::ChainInputMode::Mono => ChainOutputMode::Mono,
            _ => ChainOutputMode::Stereo,
        },
        channels: insert.send.channels.clone(),
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_enabled_chain_audio_configs(
    host: &cpal::Host,
    project: &Project,
) -> Result<HashMap<ChainId, ResolvedChainAudioConfig>> {
    let mut resolved = HashMap::new();

    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }

        let config = resolve_chain_audio_config(host, project, chain)?;
        resolved.insert(chain.id.clone(), config);
    }

    Ok(resolved)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_chain_audio_config(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
) -> Result<ResolvedChainAudioConfig> {
    let inputs = resolve_chain_inputs(host, project, chain)?;
    let outputs = resolve_chain_outputs(host, project, chain)?;

    // Validate sample rates: all inputs and outputs must agree
    let sample_rate = resolve_multi_io_sample_rate(&chain.id.0, &inputs, &outputs)?;

    let stream_signature = build_chain_stream_signature_multi(chain, &inputs, &outputs);

    Ok(ResolvedChainAudioConfig {
        inputs,
        outputs,
        sample_rate,
        stream_signature,
    })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn validate_buffer_size(
    requested: u32,
    supported: &SupportedBufferSize,
    context: &str,
) -> Result<()> {
    match supported {
        SupportedBufferSize::Range { min, max } => {
            if requested < *min || requested > *max {
                bail!(
                    "{} invalid: buffer_size_frames={} outside supported range [{}..={}]",
                    context,
                    requested,
                    min,
                    max
                );
            }
        }
        SupportedBufferSize::Unknown => {}
    }
    Ok(())
}
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn validate_channels_against_devices(project: &Project, host: &cpal::Host) -> Result<()> {
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        validate_chain_channels_against_devices(host, chain)?;
    }
    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn validate_chain_channels_against_devices(host: &cpal::Host, chain: &Chain) -> Result<()> {
    for (_, input) in chain.input_blocks() {
        for entry in &input.entries {
            validate_input_channels_against_device(host, &chain.id.0, &entry.device_id.0, &entry.channels)?;
        }
    }

    for (_, output) in chain.output_blocks() {
        for entry in &output.entries {
            validate_output_channels_against_device(host, &chain.id.0, &entry.device_id.0, &entry.channels)?;
        }
    }

    // Validate Insert block endpoints
    for (_, insert) in chain.insert_blocks() {
        if !insert.send.device_id.0.is_empty() {
            validate_output_channels_against_device(host, &chain.id.0, &insert.send.device_id.0, &insert.send.channels)?;
        }
        if !insert.return_.device_id.0.is_empty() {
            validate_input_channels_against_device(host, &chain.id.0, &insert.return_.device_id.0, &insert.return_.channels)?;
        }
    }

    Ok(())
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn validate_input_channels_against_device(
    host: &cpal::Host,
    chain_id: &str,
    device_id: &str,
    channels: &[usize],
) -> Result<()> {
    // On Linux with JACK, skip ALL ALSA channel validation — calling
    // supported_input_configs() can disturb USB audio devices regardless of
    // whether JACK is already running. JACK validates port counts at connect time.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = (host, chain_id, device_id, channels);
        log::debug!("[validate_input_channels] skipping — Linux/JACK (JACK validates at connect time)");
        return Ok(());
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        log::info!(
            "[validate_input_channels] chain='{}' device='{}' channels={:?} jack_direct=false",
            chain_id, device_id, channels
        );
        let device = find_input_device_by_id(host, device_id)?.ok_or_else(|| {
            anyhow!("chain '{}' missing input device '{}'", chain_id, device_id)
        })?;
        log::info!("[validate_input_channels] device found, querying channel capacity...");
        let total_channels = max_supported_input_channels(&device).with_context(|| {
            format!(
                "failed to resolve input channel capacity for '{}'",
                device_id
            )
        })?;
        log::info!("[validate_input_channels] device '{}' has {} channels", device_id, total_channels);
        for channel in channels {
            if *channel >= total_channels {
                bail!(
                    "chain '{}' invalid: input channel '{}' outside device range (channels={})",
                    chain_id,
                    channel,
                    total_channels
                );
            }
        }
        Ok(())
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn validate_output_channels_against_device(
    host: &cpal::Host,
    chain_id: &str,
    device_id: &str,
    channels: &[usize],
) -> Result<()> {
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = (host, chain_id, device_id, channels);
        log::debug!("[validate_output_channels] skipping — Linux/JACK (JACK validates at connect time)");
        return Ok(());
    }
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        log::info!(
            "[validate_output_channels] chain='{}' device='{}' channels={:?} jack_direct=false",
            chain_id, device_id, channels
        );
        let device = find_output_device_by_id(host, device_id)?.ok_or_else(|| {
            anyhow!("chain '{}' missing output device '{}'", chain_id, device_id)
        })?;
        log::info!("[validate_output_channels] device found, querying channel capacity...");
        let total_channels = max_supported_output_channels(&device).with_context(|| {
            format!(
                "failed to resolve output channel capacity for '{}'",
                device_id
            )
        })?;
        log::info!("[validate_output_channels] device '{}' has {} channels", device_id, total_channels);
        for channel in channels {
            if *channel >= total_channels {
                bail!(
                    "chain '{}' invalid: output channel '{}' outside device range (channels={})",
                    chain_id,
                    channel,
                    total_channels
                );
            }
        }
        Ok(())
    }
}
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn find_input_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.input_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn find_output_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.output_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
fn build_input_stream_for_input(
    chain_id: &ChainId,
    input_index: usize,
    resolved_input_device: ResolvedInputDevice,
    runtime: Arc<ChainRuntimeState>,
) -> Result<Stream> {
    log::debug!(
        "building input stream for chain '{}' input_index={}",
        chain_id.0,
        input_index
    );
    let sample_format = resolved_input_device.supported.sample_format();
    let sample_rate = resolved_input_sample_rate(&resolved_input_device);
    let buffer_size_frames = resolved_input_buffer_size_frames(&resolved_input_device);
    log::debug!(
        "input stream config: chain='{}', input_index={}, sample_rate={}, buffer_size={}, format={:?}, channels={}",
        chain_id.0, input_index, sample_rate, buffer_size_frames, sample_format, resolved_input_device.supported.channels()
    );
    let stream_config = build_stream_config(
        resolved_input_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_input_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, data, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i16::MAX as f32;
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, &converted, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = (src as f32 / u16::MAX as f32) * 2.0 - 1.0;
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, &converted, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i32::MAX as f32;
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&runtime_for_data, input_index, &converted, channels);
                    }));
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported input sample format for chain '{}': {:?}",
                chain_id.0,
                other
            );
        }
    };
    Ok(stream)
}

fn build_output_stream_for_output(
    chain_id: &ChainId,
    output_index: usize,
    resolved_output_device: ResolvedOutputDevice,
    runtime: Arc<ChainRuntimeState>,
) -> Result<Stream> {
    log::debug!(
        "building output stream for chain '{}' output_index={}",
        chain_id.0,
        output_index
    );
    let sample_format = resolved_output_device.supported.sample_format();
    let sample_rate = resolved_output_sample_rate(&resolved_output_device);
    let buffer_size_frames = resolved_output_buffer_size_frames(&resolved_output_device);
    log::debug!(
        "output stream config: chain='{}', output_index={}, sample_rate={}, buffer_size={}, format={:?}, channels={}",
        chain_id.0, output_index, sample_rate, buffer_size_frames, sample_format, resolved_output_device.supported.channels()
    );
    let stream_config = build_stream_config(
        resolved_output_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_output_device.device;
    let stream = match sample_format {
        SampleFormat::F32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, out, channels);
                    }));
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut temp = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
                    temp.resize(out.len(), 0.0);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst =
                            (*src * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut temp = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
                    temp.resize(out.len(), 0.0);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        let normalized =
                            ((*src + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32);
                        *dst = normalized as u16;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let runtime_for_data = runtime.clone();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut temp = Vec::new();
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i32], _| {
                    temp.resize(out.len(), 0.0);
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_f32(&runtime_for_data, output_index, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst = (*src * i32::MAX as f32)
                            .clamp(i32::MIN as f32, i32::MAX as f32) as i32;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        other => {
            bail!(
                "unsupported output sample format for chain '{}': {:?}",
                chain_id.0,
                other
            );
        }
    };
    Ok(stream)
}

fn build_stream_config(channels: u16, sample_rate: u32, buffer_size_frames: u32) -> StreamConfig {
    StreamConfig {
        channels,
        sample_rate,
        buffer_size: BufferSize::Fixed(buffer_size_frames),
    }
}

fn build_chain_streams(
    chain_id: &ChainId,
    resolved: ResolvedChainAudioConfig,
    runtime: Arc<ChainRuntimeState>,
) -> Result<(Vec<Stream>, Vec<Stream>)> {
    // Deduplicate input streams by device: one CPAL stream per unique device.
    // Multiple entries on the same device share the stream — the engine
    // reads each entry's channels from the same raw data buffer.
    let mut input_streams = Vec::new();
    let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, resolved_input) in resolved.inputs.into_iter().enumerate() {
        let device_key = resolved_input.device.id().map(|id| id.to_string()).unwrap_or_default();
        if !seen_devices.insert(device_key.clone()) {
            log::info!("input[{}] shares device '{}', reusing existing CPAL stream", i, device_key);
            continue;
        }
        let stream =
            build_input_stream_for_input(chain_id, i, resolved_input, runtime.clone())?;
        input_streams.push(stream);
    }

    let mut output_streams = Vec::new();
    for (j, resolved_output) in resolved.outputs.into_iter().enumerate() {
        let stream =
            build_output_stream_for_output(chain_id, j, resolved_output, runtime.clone())?;
        output_streams.push(stream);
    }

    Ok((input_streams, output_streams))
}

fn build_active_chain_runtime(
    chain_id: &ChainId,
    #[allow(unused_variables)] chain: &Chain,
    resolved: ResolvedChainAudioConfig,
    runtime: Arc<ChainRuntimeState>,
) -> Result<ActiveChainRuntime> {
    log::info!("building active chain runtime for '{}', sample_rate={}", chain_id.0, resolved.sample_rate);
    let stream_signature = resolved.stream_signature.clone();

    // On Linux with JACK: use the jack crate directly for zero-overhead audio.
    // This bypasses CPAL entirely — the JACK process callback runs in the
    // real-time thread with no extra buffering.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            log::info!("JACK detected — using direct JACK backend (bypassing CPAL)");
            let (jack_client, dsp_worker) = build_jack_direct_chain(chain_id, chain, runtime)?;
            return Ok(ActiveChainRuntime {
                stream_signature,
                _input_streams: Vec::new(),
                _output_streams: Vec::new(),
                _jack_client: Some(jack_client),
                _dsp_worker: Some(dsp_worker),
            });
        }
    }

    let (input_streams, output_streams) = build_chain_streams(chain_id, resolved, runtime)?;
    for stream in &input_streams {
        stream.play()?;
    }
    for stream in &output_streams {
        stream.play()?;
    }
    log::info!(
        "audio streams started for chain '{}': {} input(s), {} output(s)",
        chain_id.0,
        input_streams.len(),
        output_streams.len()
    );
    Ok(ActiveChainRuntime {
        stream_signature,
        _input_streams: input_streams,
        _output_streams: output_streams,
        #[cfg(all(target_os = "linux", feature = "jack"))]
        _jack_client: None,
        #[cfg(all(target_os = "linux", feature = "jack"))]
        _dsp_worker: None,
    })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn build_chain_stream_signature_multi(
    chain: &Chain,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
) -> ChainStreamSignature {
    let chain_input_entries: Vec<&InputEntry> = chain.input_blocks()
        .into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .collect();
    let input_sigs: Vec<InputStreamSignature> = if !chain_input_entries.is_empty() {
        chain_input_entries
            .iter()
            .zip(inputs.iter())
            .map(|(ci, ri)| InputStreamSignature {
                device_id: ci.device_id.0.clone(),
                channels: ci.channels.clone(),
                stream_channels: ri.supported.channels(),
                sample_rate: resolved_input_sample_rate(ri),
                buffer_size_frames: resolved_input_buffer_size_frames(ri),
            })
            .collect()
    } else {
        inputs
            .iter()
            .map(|ri| InputStreamSignature {
                device_id: String::new(),
                channels: Vec::new(),
                stream_channels: ri.supported.channels(),
                sample_rate: resolved_input_sample_rate(ri),
                buffer_size_frames: resolved_input_buffer_size_frames(ri),
            })
            .collect()
    };

    let chain_output_entries: Vec<&OutputEntry> = chain.output_blocks()
        .into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .collect();
    let output_sigs: Vec<OutputStreamSignature> = if !chain_output_entries.is_empty() {
        chain_output_entries
            .iter()
            .zip(outputs.iter())
            .map(|(co, ro)| OutputStreamSignature {
                device_id: co.device_id.0.clone(),
                channels: co.channels.clone(),
                stream_channels: ro.supported.channels(),
                sample_rate: resolved_output_sample_rate(ro),
                buffer_size_frames: resolved_output_buffer_size_frames(ro),
            })
            .collect()
    } else {
        outputs
            .iter()
            .map(|ro| OutputStreamSignature {
                device_id: String::new(),
                channels: Vec::new(),
                stream_channels: ro.supported.channels(),
                sample_rate: resolved_output_sample_rate(ro),
                buffer_size_frames: resolved_output_buffer_size_frames(ro),
            })
            .collect()
    };

    ChainStreamSignature {
        inputs: input_sigs,
        outputs: output_sigs,
    }
}

fn resolved_input_sample_rate(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

fn resolved_output_sample_rate(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.sample_rate)
        .unwrap_or_else(|| resolved.supported.sample_rate())
}

fn resolved_input_buffer_size_frames(resolved: &ResolvedInputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

fn resolved_output_buffer_size_frames(resolved: &ResolvedOutputDevice) -> u32 {
    resolved
        .settings
        .as_ref()
        .map(|settings| settings.buffer_size_frames)
        .unwrap_or(256)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn required_channel_count(channels: &[usize]) -> usize {
    channels
        .iter()
        .copied()
        .max()
        .map(|channel| channel + 1)
        .unwrap_or(0)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn select_supported_stream_config(
    default_config: &SupportedStreamConfig,
    supported_ranges: &[SupportedStreamConfigRange],
    requested_sample_rate: Option<u32>,
    required_channels: usize,
    context: &str,
) -> Result<SupportedStreamConfig> {
    let target_sample_rate = requested_sample_rate.unwrap_or_else(|| default_config.sample_rate());
    let default_format = default_config.sample_format();

    let best = supported_ranges
        .iter()
        .filter(|range| range.channels() as usize >= required_channels)
        .filter_map(|range| range.try_with_sample_rate(target_sample_rate))
        .min_by_key(|config| {
            (
                (config.channels() as usize != required_channels) as u8,
                (config.sample_format() != default_format) as u8,
                (config.channels() as usize).saturating_sub(required_channels),
            )
        });

    best.ok_or_else(|| {
        anyhow!(
            "{} invalid: no supported config for sample_rate={} with at least {} channels",
            context,
            target_sample_rate,
            required_channels
        )
    })
}

#[cfg(test)]
fn resolve_chain_runtime_sample_rate(
    chain_id: &str,
    input: &SupportedStreamConfig,
    output: &SupportedStreamConfig,
) -> Result<f32> {
    if input.sample_rate() != output.sample_rate() {
        bail!(
            "chain '{}' invalid: input sample_rate={} differs from output sample_rate={}",
            chain_id,
            input.sample_rate(),
            output.sample_rate()
        );
    }

    Ok(input.sample_rate() as f32)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn resolve_multi_io_sample_rate(
    chain_id: &str,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
) -> Result<f32> {
    let mut rate: Option<u32> = None;
    for ri in inputs {
        let sr = resolved_input_sample_rate(ri);
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across inputs ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    for ro in outputs {
        let sr = resolved_output_sample_rate(ro);
        if let Some(prev) = rate {
            if prev != sr {
                bail!(
                    "chain '{}' invalid: mismatched sample rates across I/O ({} vs {})",
                    chain_id,
                    prev,
                    sr
                );
            }
        }
        rate = Some(sr);
    }
    rate.map(|r| r as f32)
        .ok_or_else(|| anyhow!("chain '{}' has no inputs or outputs", chain_id))
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn max_supported_input_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = match device.supported_input_configs() {
        Ok(configs) => {
            let max = configs.map(|config| config.channels() as usize).max();
            log::info!("[max_supported_input_channels] supported_input_configs max={:?}", max);
            max
        }
        Err(e) => {
            log::warn!("[max_supported_input_channels] supported_input_configs error: {}", e);
            return Err(e.into());
        }
    };
    let default_channels = match device.default_input_config() {
        Ok(config) => {
            let ch = config.channels() as usize;
            log::info!("[max_supported_input_channels] default_input_config channels={}", ch);
            Some(ch)
        }
        Err(e) => {
            log::info!("[max_supported_input_channels] default_input_config error: {}", e);
            None
        }
    };
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn max_supported_output_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = match device.supported_output_configs() {
        Ok(configs) => {
            let max = configs.map(|config| config.channels() as usize).max();
            log::info!("[max_supported_output_channels] supported_output_configs max={:?}", max);
            max
        }
        Err(e) => {
            log::warn!("[max_supported_output_channels] supported_output_configs error: {}", e);
            return Err(e.into());
        }
    };
    let default_channels = match device.default_output_config() {
        Ok(config) => {
            let ch = config.channels() as usize;
            log::info!("[max_supported_output_channels] default_output_config channels={}", ch);
            Some(ch)
        }
        Err(e) => {
            log::info!("[max_supported_output_channels] default_output_config error: {}", e);
            None
        }
    };
    max_supported_channels(default_channels, max_supported)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn max_supported_channels(
    default_channels: Option<usize>,
    max_supported_channels: Option<usize>,
) -> Result<usize> {
    max_supported_channels
        .or(default_channels)
        .ok_or_else(|| anyhow!("device exposes no supported channels"))
}

#[cfg(test)]
mod tests {
    use super::{build_stream_config, resolve_chain_runtime_sample_rate, AudioDeviceDescriptor, ProjectRuntimeController};
    use cpal::BufferSize;
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    use super::{max_supported_channels, required_channel_count, select_supported_stream_config, validate_buffer_size};
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    fn supported_range(
        channels: u16,
        min_sample_rate: u32,
        max_sample_rate: u32,
    ) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(
            channels,
            min_sample_rate,
            max_sample_rate,
            SupportedBufferSize::Range { min: 64, max: 1024 },
            SampleFormat::F32,
        )
    }

    // ── AudioDeviceDescriptor ───────────────────────────────────────

    #[test]
    fn audio_device_descriptor_construction_stores_fields() {
        let desc = AudioDeviceDescriptor {
            id: "coreaudio:abc123".to_string(),
            name: "USB Audio Interface".to_string(),
            channels: 2,
        };
        assert_eq!(desc.id, "coreaudio:abc123");
        assert_eq!(desc.name, "USB Audio Interface");
        assert_eq!(desc.channels, 2);
    }

    #[test]
    fn audio_device_descriptor_equality_same_values_returns_true() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        let b = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn audio_device_descriptor_equality_different_id_returns_false() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        let b = AudioDeviceDescriptor {
            id: "dev2".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn audio_device_descriptor_clone_produces_equal_copy() {
        let original = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "My Device".to_string(),
            channels: 8,
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn audio_device_descriptor_debug_format_contains_fields() {
        let desc = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Test".to_string(),
            channels: 2,
        };
        let debug = format!("{:?}", desc);
        assert!(debug.contains("dev1"));
        assert!(debug.contains("Test"));
    }

    // ── select_supported_stream_config ──────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_accepts_non_default_sample_rate_when_device_supports_it() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(2, 44_100, 96_000),
            supported_range(1, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(44_100),
            2,
            "test-device",
        )
        .expect("supported non-default sample rate should resolve");

        assert_eq!(resolved.sample_rate(), 44_100);
        assert_eq!(resolved.channels(), 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_no_requested_rate_uses_default() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 96_000)];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            None,
            2,
            "test-device",
        )
        .expect("should use default sample rate");

        assert_eq!(resolved.sample_rate(), 48_000);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_unsupported_rate_returns_error() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 44_100)];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(96_000),
            2,
            "test-device",
        );

        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_insufficient_channels_returns_error() {
        let default_config = supported_range(1, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(1, 44_100, 96_000)];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            4,
            "test-device",
        );

        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn select_supported_stream_config_picks_minimum_channels_matching() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(8, 44_100, 96_000),
            supported_range(2, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        )
        .unwrap();

        assert_eq!(resolved.channels(), 2);
    }

    // ── resolve_chain_runtime_sample_rate ────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn resolve_chain_runtime_sample_rate_rejects_mismatched_input_and_output_sample_rates() {
        let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();

        let error = resolve_chain_runtime_sample_rate("chain:0", &input, &output)
            .expect_err("mismatched rates should fail");

        assert!(error.to_string().contains("sample_rate"));
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn resolve_chain_runtime_sample_rate_matching_rates_returns_rate() {
        let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let output = supported_range(2, 48_000, 48_000).with_max_sample_rate();

        let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();

        assert_eq!(rate, 48_000.0);
    }

    // ── max_supported_channels ──────────────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_prefers_supported_capacity_over_default() {
        let resolved =
            max_supported_channels(Some(2), Some(8)).expect("supported channels should resolve");

        assert_eq!(resolved, 8);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_uses_default_when_supported_list_is_empty() {
        let resolved =
            max_supported_channels(Some(2), None).expect("default channels should resolve");

        assert_eq!(resolved, 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_both_none_returns_error() {
        let result = max_supported_channels(None, None);
        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn max_supported_channels_only_supported_uses_supported() {
        let resolved =
            max_supported_channels(None, Some(6)).expect("should use supported channels");
        assert_eq!(resolved, 6);
    }

    // ── required_channel_count ──────────────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_empty_returns_zero() {
        assert_eq!(required_channel_count(&[]), 0);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_single_channel_zero_returns_one() {
        assert_eq!(required_channel_count(&[0]), 1);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_stereo_returns_two() {
        assert_eq!(required_channel_count(&[0, 1]), 2);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_non_contiguous_returns_max_plus_one() {
        assert_eq!(required_channel_count(&[0, 3, 7]), 8);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn required_channel_count_single_high_channel_returns_correct() {
        assert_eq!(required_channel_count(&[5]), 6);
    }

    // ── validate_buffer_size ────────────────────────────────────────

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_within_range_succeeds() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(256, &supported, "test");
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_at_min_boundary_succeeds() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(64, &supported, "test");
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_at_max_boundary_succeeds() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(1024, &supported, "test");
        assert!(result.is_ok());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_below_min_returns_error() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(32, &supported, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("outside supported range"));
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_above_max_returns_error() {
        let supported = SupportedBufferSize::Range { min: 64, max: 1024 };
        let result = validate_buffer_size(2048, &supported, "test");
        assert!(result.is_err());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    #[test]
    fn validate_buffer_size_unknown_always_succeeds() {
        let supported = SupportedBufferSize::Unknown;
        let result = validate_buffer_size(9999, &supported, "test");
        assert!(result.is_ok());
    }

    // ── build_stream_config ─────────────────────────────────────────

    #[test]
    fn build_stream_config_sets_channels_and_rate() {
        let config = build_stream_config(2, 48_000, 256);
        assert_eq!(config.channels, 2);
        assert_eq!(config.sample_rate, 48_000);
        assert_eq!(config.buffer_size, BufferSize::Fixed(256));
    }

    #[test]
    fn build_stream_config_mono_128_buffer() {
        let config = build_stream_config(1, 44_100, 128);
        assert_eq!(config.channels, 1);
        assert_eq!(config.sample_rate, 44_100);
        assert_eq!(config.buffer_size, BufferSize::Fixed(128));
    }

    // ── build_stream_config edge cases ──────────────────────────────────────

    #[test]
    fn build_stream_config_high_sample_rate() {
        let config = build_stream_config(2, 96_000, 512);
        assert_eq!(config.channels, 2);
        assert_eq!(config.sample_rate, 96_000);
        assert_eq!(config.buffer_size, BufferSize::Fixed(512));
    }

    #[test]
    fn build_stream_config_large_buffer() {
        let config = build_stream_config(8, 48_000, 1024);
        assert_eq!(config.channels, 8);
        assert_eq!(config.buffer_size, BufferSize::Fixed(1024));
    }

    // ── validate_buffer_size edge cases ─────────────────────────────────────

    #[test]
    fn validate_buffer_size_exactly_one_element_range_succeeds() {
        let supported = SupportedBufferSize::Range { min: 256, max: 256 };
        let result = validate_buffer_size(256, &supported, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_buffer_size_exactly_one_element_range_rejects_other() {
        let supported = SupportedBufferSize::Range { min: 256, max: 256 };
        let result = validate_buffer_size(128, &supported, "test");
        assert!(result.is_err());
    }

    // ── required_channel_count more edge cases ──────────────────────────────

    #[test]
    fn required_channel_count_duplicate_channels() {
        // Duplicate channels should still return max+1
        assert_eq!(required_channel_count(&[0, 0, 0]), 1);
    }

    #[test]
    fn required_channel_count_unsorted_channels() {
        assert_eq!(required_channel_count(&[3, 1, 5, 2]), 6);
    }

    // ── max_supported_channels additional tests ─────────────────────────────

    #[test]
    fn max_supported_channels_same_default_and_supported() {
        let resolved = max_supported_channels(Some(4), Some(4)).unwrap();
        assert_eq!(resolved, 4);
    }

    #[test]
    fn max_supported_channels_zero_default_with_some_supported() {
        let resolved = max_supported_channels(Some(0), Some(2)).unwrap();
        assert_eq!(resolved, 2);
    }

    // ── select_supported_stream_config additional tests ─────────────────────

    #[test]
    fn select_supported_stream_config_empty_ranges_returns_error() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported: Vec<SupportedStreamConfigRange> = vec![];

        let result = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        );

        assert!(result.is_err(), "empty ranges should return error");
    }

    #[test]
    fn select_supported_stream_config_zero_channels_required() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![supported_range(2, 44_100, 96_000)];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            0,
            "test-device",
        )
        .expect("zero required channels should match any range");

        assert!(resolved.channels() >= 1);
    }

    #[test]
    fn select_supported_stream_config_prefers_exact_channel_match() {
        let default_config = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let supported = vec![
            supported_range(4, 44_100, 96_000),
            supported_range(2, 44_100, 96_000),
            supported_range(8, 44_100, 96_000),
        ];

        let resolved = select_supported_stream_config(
            &default_config,
            &supported,
            Some(48_000),
            2,
            "test-device",
        )
        .unwrap();

        assert_eq!(resolved.channels(), 2, "should prefer exact channel count");
    }

    // ── resolve_chain_runtime_sample_rate tests ─────────────────────────────

    #[test]
    fn resolve_chain_runtime_sample_rate_high_rate_matching() {
        let input = supported_range(2, 96_000, 96_000).with_max_sample_rate();
        let output = supported_range(2, 96_000, 96_000).with_max_sample_rate();
        let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();
        assert_eq!(rate, 96_000.0);
    }

    #[test]
    fn resolve_chain_runtime_sample_rate_low_rate_matching() {
        let input = supported_range(2, 44_100, 44_100).with_max_sample_rate();
        let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();
        let rate = resolve_chain_runtime_sample_rate("chain:0", &input, &output).unwrap();
        assert_eq!(rate, 44_100.0);
    }

    // ── AudioDeviceDescriptor additional tests ──────────────────────────────

    #[test]
    fn audio_device_descriptor_different_channels_not_equal() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 2,
        };
        let b = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device".to_string(),
            channels: 4,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn audio_device_descriptor_different_name_not_equal() {
        let a = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device A".to_string(),
            channels: 2,
        };
        let b = AudioDeviceDescriptor {
            id: "dev1".to_string(),
            name: "Device B".to_string(),
            channels: 2,
        };
        assert_ne!(a, b);
    }

    #[test]
    fn audio_device_descriptor_zero_channels() {
        let desc = AudioDeviceDescriptor {
            id: "dev0".to_string(),
            name: "Null".to_string(),
            channels: 0,
        };
        assert_eq!(desc.channels, 0);
    }

    // ── InputStreamSignature / OutputStreamSignature equality ────────────────

    #[test]
    fn input_stream_signature_equality() {
        use super::InputStreamSignature;
        let a = InputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn input_stream_signature_different_rate_not_equal() {
        use super::InputStreamSignature;
        let a = InputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = InputStreamSignature {
            sample_rate: 44_100,
            ..a.clone()
        };
        assert_ne!(a, b);
    }

    #[test]
    fn output_stream_signature_equality() {
        use super::OutputStreamSignature;
        let a = OutputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn output_stream_signature_different_channels_not_equal() {
        use super::OutputStreamSignature;
        let a = OutputStreamSignature {
            device_id: "dev1".to_string(),
            channels: vec![0, 1],
            stream_channels: 2,
            sample_rate: 48_000,
            buffer_size_frames: 256,
        };
        let b = OutputStreamSignature {
            channels: vec![0],
            ..a.clone()
        };
        assert_ne!(a, b);
    }

    // ── ChainStreamSignature equality ───────────────────────────────────────

    #[test]
    fn chain_stream_signature_equality() {
        use super::{ChainStreamSignature, InputStreamSignature, OutputStreamSignature};
        let a = ChainStreamSignature {
            inputs: vec![InputStreamSignature {
                device_id: "dev1".to_string(),
                channels: vec![0],
                stream_channels: 1,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
            outputs: vec![OutputStreamSignature {
                device_id: "dev2".to_string(),
                channels: vec![0, 1],
                stream_channels: 2,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
        };
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn chain_stream_signature_different_inputs_not_equal() {
        use super::{ChainStreamSignature, InputStreamSignature};
        let a = ChainStreamSignature {
            inputs: vec![InputStreamSignature {
                device_id: "dev1".to_string(),
                channels: vec![0],
                stream_channels: 1,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
            outputs: vec![],
        };
        let b = ChainStreamSignature {
            inputs: vec![InputStreamSignature {
                device_id: "dev2".to_string(),
                channels: vec![0],
                stream_channels: 1,
                sample_rate: 48_000,
                buffer_size_frames: 256,
            }],
            outputs: vec![],
        };
        assert_ne!(a, b);
    }

    // ── is_asio_host (non-Windows always returns false) ─────────────────────

    #[test]
    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    fn is_asio_host_returns_false_on_non_windows() {
        use super::is_asio_host;
        let host = cpal::default_host();
        assert!(!is_asio_host(&host), "non-Windows host should not be ASIO");
    }

    // ── insert_return_as_input_entry ────────────────────────────────────────

    #[test]
    fn insert_return_as_input_entry_copies_return_fields() {
        use super::insert_return_as_input_entry;
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::ChainInputMode;
        use domain::ids::DeviceId;

        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![2, 3],
            },
        };
        let entry = insert_return_as_input_entry(&insert);
        assert_eq!(entry.device_id.0, "return");
        assert_eq!(entry.channels, vec![2, 3]);
    }

    // ── insert_send_as_output_entry ─────────────────────────────────────────

    #[test]
    fn insert_send_as_output_entry_mono_becomes_mono() {
        use super::insert_send_as_output_entry;
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::{ChainInputMode, ChainOutputMode};
        use domain::ids::DeviceId;

        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert_eq!(entry.device_id.0, "send");
        assert!(matches!(entry.mode, ChainOutputMode::Mono));
    }

    #[test]
    fn insert_send_as_output_entry_stereo_becomes_stereo() {
        use super::insert_send_as_output_entry;
        use project::block::{InsertBlock, InsertEndpoint};
        use project::chain::{ChainInputMode, ChainOutputMode};
        use domain::ids::DeviceId;

        let insert = InsertBlock {
            model: "external_loop".into(),
            send: InsertEndpoint {
                device_id: DeviceId("send".into()),
                mode: ChainInputMode::Stereo,
                channels: vec![0, 1],
            },
            return_: InsertEndpoint {
                device_id: DeviceId("return".into()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            },
        };
        let entry = insert_send_as_output_entry(&insert);
        assert!(matches!(entry.mode, ChainOutputMode::Stereo));
    }

    #[test]
    fn is_healthy_returns_true_when_no_chains_active() {
        let controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
        };
        assert!(controller.is_healthy());
    }

    #[test]
    fn is_running_returns_false_when_no_chains() {
        let controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
        };
        assert!(!controller.is_running());
    }

    // ── Regression tests for issue #294: stale JACK client on chain reconfigure ──
    //
    // Reconfiguring input channels on an active chain (e.g. unchecking a channel
    // in a stereo input) used to leave the previous JACK client alive while the
    // replacement client was being built, because HashMap::insert only dropped
    // the old ActiveChainRuntime AFTER constructing the new one. On JACK, the
    // new client would get a suffixed name while connect_ports_by_name still
    // used the literal (unsuffixed) name — so the connections bound to the
    // OLD client's ports, which then vanished when the old client was finally
    // dropped, leaving the new client orphaned and audio silent.
    //
    // The fix tears down the existing ActiveChainRuntime BEFORE building the
    // replacement (teardown_active_chain_for_rebuild), mirroring the pattern
    // in remove_chain. These tests cover the teardown helper directly; the
    // end-to-end "audio still flows after channel toggle" behavior is
    // verifiable only on real JACK hardware and is exercised manually on the
    // Orange Pi during regression testing.

    #[test]
    fn teardown_active_chain_for_rebuild_drops_entry_when_present() {
        let chain_id = super::ChainId("chain:0".into());
        let mut controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
        };
        controller.active_chains.insert(chain_id.clone(), super::ActiveChainRuntime {
            stream_signature: super::ChainStreamSignature { inputs: vec![], outputs: vec![] },
            _input_streams: vec![],
            _output_streams: vec![],
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _jack_client: None,
            #[cfg(all(target_os = "linux", feature = "jack"))]
            _dsp_worker: None,
        });
        assert!(controller.active_chains.contains_key(&chain_id));

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(!controller.active_chains.contains_key(&chain_id),
            "active_chains entry must be removed so the old JACK client/DSP worker are dropped \
             before a replacement is built");
    }

    #[test]
    fn teardown_active_chain_for_rebuild_is_noop_when_chain_absent() {
        let chain_id = super::ChainId("chain:missing".into());
        let mut controller = ProjectRuntimeController {
            runtime_graph: super::RuntimeGraph {
                chains: std::collections::HashMap::new(),
            },
            active_chains: std::collections::HashMap::new(),
        };

        controller.teardown_active_chain_for_rebuild(&chain_id);

        assert!(controller.active_chains.is_empty());
    }
}
