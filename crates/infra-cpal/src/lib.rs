use anyhow::{anyhow, bail, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, SampleFormat, Stream, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
    SupportedStreamConfigRange,
};
use std::sync::OnceLock;

// ── Cached host ─────────────────────────────────────────────────────────────
// select_host() was called 8+ times per session (each creating a new CPAL
// host, which on JACK means a new client connection).  Cache it once.

static HOST: OnceLock<cpal::Host> = OnceLock::new();

fn get_host() -> &'static cpal::Host {
    HOST.get_or_init(create_host)
}

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

    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        // When JACK is running, streaming is handled by the direct JACK backend
        // (jack crate) in build_active_chain_runtime(). Return ALSA for device
        // resolution — it can still read device capabilities even while JACK
        // holds the hardware.
        if jack_server_is_running() {
            log::info!("Audio host: ALSA (JACK streaming via direct jack crate backend)");
            return cpal::default_host();
        }
    }

    cpal::default_host()
}

/// Read the physical hardware name managed by JACK from /proc/asound/cards.
///
/// JACK abstracts hardware so CPAL only exposes "cpal_client_in/out" as device
/// names. This function reads the kernel's card list to find the actual hardware
/// name (e.g. "Scarlett 2i2 4th Gen") and returns it for display purposes.
/// Returns None if no USB audio card is found or the file is unreadable.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_hardware_name() -> Option<String> {
    let content = std::fs::read_to_string("/proc/asound/cards").ok()?;
    for line in content.lines() {
        // Lines look like: " 1 [Gen ]: USB-Audio - Scarlett 2i2 4th Gen"
        if line.contains("USB-Audio") || line.contains("USB Audio") {
            if let Some(pos) = line.find(" - ") {
                let name = line[pos + 3..].trim().to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

/// Select the audio host for device enumeration.
///
/// Returns true when the JACK server is running and reachable, determined by
/// a non-blocking directory scan. Safe to call from the UI thread.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_server_is_running() -> bool {
    // jackd creates a Unix socket at /dev/shm/jack_default_{uid}_0.
    // Scanning /dev/shm for this pattern is instant and non-blocking.
    std::fs::read_dir("/dev/shm").ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("jack_default_") && s.ends_with("_0")
            })
        })
        .unwrap_or(false)
}

/// Snapshot hardware audio status (JACK + ALSA cards + USB audio devices) and
/// emit it as a single structured log line. Safe to call from the UI thread on
/// any platform: the Linux implementation is pure filesystem scans (no JACK
/// client, no ALSA open, no CPAL host); other platforms are no-ops.
///
/// Intended to be invoked after every user mutation so that `journalctl -fu
/// openrig` can correlate UI actions with audio hardware state changes (e.g.
/// Scarlett disconnects on RK3588 xHCI).
pub fn log_audio_status(context: &str) {
    #[cfg(target_os = "linux")]
    {
        let alsa_cards = read_proc_asound_cards_for_status();
        let usb_audio = read_usb_audio_devices_for_status();

        #[cfg(feature = "jack")]
        let jack_running = jack_server_is_running();
        #[cfg(not(feature = "jack"))]
        let jack_running = false;

        log::info!(
            "[AUDIO-STATUS] ctx='{}' jack_running={} alsa_cards=[{}] usb_audio=[{}]",
            context,
            jack_running,
            alsa_cards.join(" | "),
            usb_audio.join(" | "),
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = context;
    }
}

#[cfg(target_os = "linux")]
fn read_proc_asound_cards_for_status() -> Vec<String> {
    std::fs::read_to_string("/proc/asound/cards")
        .ok()
        .map(|content| {
            content
                .lines()
                .filter_map(|line| {
                    // Only lines starting with a card index (e.g. " 0 [Headphones     ]: ...").
                    let trimmed = line.trim_start();
                    let first = trimmed.chars().next()?;
                    if first.is_ascii_digit() {
                        Some(trimmed.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
fn read_usb_audio_devices_for_status() -> Vec<String> {
    let root = std::path::Path::new("/sys/bus/usb/devices");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let node = entry.file_name().to_string_lossy().into_owned();
        // Skip interface nodes (they contain ':'); we only want device nodes.
        if node.contains(':') {
            continue;
        }
        // Keep only devices that expose at least one audio-class interface (01).
        let is_audio = std::fs::read_dir(&path)
            .ok()
            .map(|sub| {
                sub.flatten().any(|s| {
                    let sn = s.file_name();
                    let sn = sn.to_string_lossy();
                    if !sn.contains(':') {
                        return false;
                    }
                    std::fs::read_to_string(s.path().join("bInterfaceClass"))
                        .map(|v| v.trim() == "01")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !is_audio {
            continue;
        }

        let vendor = std::fs::read_to_string(path.join("idVendor"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let product = std::fs::read_to_string(path.join("idProduct"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let product_name = std::fs::read_to_string(path.join("product"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "?".to_string());

        out.push(format!("{}:{} {} @{}", vendor, product, product_name, node));
    }
    out
}

/// Select the CPAL audio host for device enumeration (non-JACK path only).
///
/// When JACK is running, callers should use `jack_enumerate_*_devices()` instead
/// of this function — CPAL's JACK backend must never be instantiated.
fn select_host_for_enumeration() -> &'static cpal::Host {
    // CPAL JACK host is never created — it crashes the JACK server.
    // When JACK is running, list_*_device_descriptors() bypass this function entirely.
    static ENUM_HOST: OnceLock<cpal::Host> = OnceLock::new();
    ENUM_HOST.get_or_init(cpal::default_host)
}

/// Cached JACK server metadata.
///
/// Populated by a SINGLE transient JACK client on first access and reused for
/// every enumeration, sample-rate query and chain config resolution until the
/// process exits or jackd is restarted. This eliminates the churn of opening
/// a fresh JACK client on every UI interaction (device picker, block change,
/// chain resync, etc.) — which on fragile USB audio stacks (Rockchip xHCI +
/// Scarlett Gen 4) has been correlated with xHCI host controller resets and
/// interface disconnects.
#[cfg(all(target_os = "linux", feature = "jack"))]
#[derive(Clone)]
struct JackMeta {
    sample_rate: u32,
    buffer_size: u32,
    capture_port_count: usize,
    playback_port_count: usize,
    hw_name: String,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
static JACK_META_CACHE: std::sync::Mutex<Option<JackMeta>> = std::sync::Mutex::new(None);

/// Return cached JACK metadata, creating a single transient JACK client the
/// first time. All other call sites that used to open their own client now go
/// through this function, so the process opens at most ONE enumeration client
/// during its lifetime (plus the per-chain AsyncClient used for real-time
/// processing).
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_meta() -> Result<JackMeta> {
    if let Some(cached) = JACK_META_CACHE.lock().unwrap().clone() {
        log::trace!("jack meta: cache hit");
        return Ok(cached);
    }

    log::debug!("jack meta: cache miss, creating single transient JACK client");
    let (client, _) = jack::Client::new(
        "openrig_meta",
        jack::ClientOptions::NO_START_SERVER,
    ).map_err(|e| anyhow!("failed to connect to JACK for metadata: {:?}", e))?;

    let capture_ports = client.ports(Some("system:capture_"), None, jack::PortFlags::IS_OUTPUT);
    let playback_ports = client.ports(Some("system:playback_"), None, jack::PortFlags::IS_INPUT);
    let meta = JackMeta {
        sample_rate: client.sample_rate() as u32,
        buffer_size: client.buffer_size(),
        capture_port_count: capture_ports.len(),
        playback_port_count: playback_ports.len(),
        hw_name: jack_hardware_name().unwrap_or_else(|| "JACK Audio".to_string()),
    };
    drop(client);

    *JACK_META_CACHE.lock().unwrap() = Some(meta.clone());
    log::debug!(
        "jack meta: cached sr={} buf={} in={} out={} hw='{}'",
        meta.sample_rate, meta.buffer_size,
        meta.capture_port_count, meta.playback_port_count, meta.hw_name
    );
    Ok(meta)
}

/// Invalidate the cached JACK metadata so the next `jack_meta()` call
/// reconnects to the JACK server.  Called when device enumeration fails,
/// indicating the JACK server may have restarted (e.g. after a USB
/// disconnect/reconnect cycle).
#[cfg(all(target_os = "linux", feature = "jack"))]
fn invalidate_jack_meta_cache() {
    if let Ok(mut cache) = JACK_META_CACHE.lock() {
        if cache.is_some() {
            log::info!("jack meta: cache invalidated — will reconnect on next enumeration");
            *cache = None;
        }
    }
}

/// Enumerate input devices directly via cached JACK metadata.
///
/// Returns one AudioDeviceDescriptor per hardware interface with the real
/// hardware name from /proc/asound/cards. Never creates a new JACK client on
/// its own — relies on `jack_meta()` which opens at most one client per
/// process lifetime.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_enumerate_input_devices() -> Result<Vec<AudioDeviceDescriptor>> {
    let meta = jack_meta()?;
    if meta.capture_port_count == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![AudioDeviceDescriptor {
        id: "jack:system".to_string(),
        name: format!("{} (JACK)", meta.hw_name),
        channels: meta.capture_port_count,
    }])
}

/// Enumerate output devices directly via cached JACK metadata.
/// See `jack_enumerate_input_devices` for rationale.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_enumerate_output_devices() -> Result<Vec<AudioDeviceDescriptor>> {
    let meta = jack_meta()?;
    if meta.playback_port_count == 0 {
        return Ok(Vec::new());
    }
    Ok(vec![AudioDeviceDescriptor {
        id: "jack:system".to_string(),
        name: format!("{} (JACK)", meta.hw_name),
        channels: meta.playback_port_count,
    }])
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
/// (e.g. in Focusrite Control) — project settings must be ignored for those devices.
#[cfg(target_os = "windows")]
fn is_asio_host(host: &cpal::Host) -> bool {
    host.id() == cpal::HostId::Asio
}

#[cfg(not(target_os = "windows"))]
fn is_asio_host(_host: &cpal::Host) -> bool {
    false
}

// is_jack_host() removed — CPAL JACK host is never created.
// Use using_jack_direct() to check if the direct JACK backend is active.

use domain::ids::ChainId;
use engine::runtime::{process_input_f32, process_output_f32, RuntimeGraph, ChainRuntimeState};
use engine;
use project::device::DeviceSettings;
use project::project::Project;
use project::block::{AudioBlockKind, InputEntry, InsertBlock, OutputEntry};
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
    stream_signature: ChainStreamSignature,
    _input_streams: Vec<Stream>,
    _output_streams: Vec<Stream>,
    #[cfg(all(target_os = "linux", feature = "jack"))]
    _jack_client: Option<jack::AsyncClient<(), JackProcessHandler>>,
}

/// Direct JACK process handler — runs in the JACK real-time thread with zero
/// extra buffering. Reads from JACK input ports, interleaves into the engine,
/// then deinterleaves engine output into JACK output ports.
#[cfg(all(target_os = "linux", feature = "jack"))]
struct JackProcessHandler {
    input_ports: Vec<jack::Port<jack::AudioIn>>,
    output_ports: Vec<jack::Port<jack::AudioOut>>,
    runtime: Arc<ChainRuntimeState>,
}

#[cfg(all(target_os = "linux", feature = "jack"))]
impl jack::ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        let n_frames = ps.n_frames() as usize;

        // --- Input: read from JACK ports, interleave, feed engine ---
        let total_in_ports = self.input_ports.len();
        if total_in_ports > 0 {
            let mut interleaved = vec![0.0f32; n_frames * total_in_ports];
            for (ch, port) in self.input_ports.iter().enumerate() {
                let port_data = port.as_slice(ps);
                for frame in 0..n_frames {
                    interleaved[frame * total_in_ports + ch] = port_data[frame];
                }
            }
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                process_input_f32(&self.runtime, 0, &interleaved, total_in_ports);
            }));
        }

        // --- Output: pull from engine, deinterleave into JACK ports ---
        let total_out_ports = self.output_ports.len();
        if total_out_ports > 0 {
            let mut interleaved = vec![0.0f32; n_frames * total_out_ports];
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                process_output_f32(&self.runtime, 0, &mut interleaved, total_out_ports);
            }));
            for (ch, port) in self.output_ports.iter_mut().enumerate() {
                let port_data = port.as_mut_slice(ps);
                for frame in 0..n_frames {
                    port_data[frame] = interleaved[frame * total_out_ports + ch];
                }
            }
        }

        jack::Control::Continue
    }
}

#[cfg(all(target_os = "linux", feature = "jack"))]
fn build_jack_direct_chain(
    chain_id: &ChainId,
    chain: &Chain,
    runtime: Arc<ChainRuntimeState>,
) -> Result<jack::AsyncClient<(), JackProcessHandler>> {
    let client_name = format!("openrig_{}", chain_id.0);
    let (client, _status) = jack::Client::new(
        &client_name,
        jack::ClientOptions::NO_START_SERVER,
    ).map_err(|e| anyhow!("failed to create JACK client: {:?}", e))?;

    let sample_rate = client.sample_rate() as f32;
    log::info!(
        "JACK direct: client '{}', sample_rate={}, buffer_size={}",
        client_name, sample_rate, client.buffer_size()
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

    // Determine how many input ports we need (max channel index + 1)
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

    let handler = JackProcessHandler {
        input_ports,
        output_ports,
        runtime,
    };

    let active_client = client.activate_async((), handler)
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
        "JACK direct: chain '{}' active with {} input(s), {} output(s)",
        chain_id.0, max_in_ch, max_out_ch
    );

    Ok(active_client)
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

    #[allow(unreachable_code)]
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

/// On Linux/ALSA, cpal lists all logical devices (equivalent to `aplay -L`),
/// which includes dozens of virtual entries per card (surround51, iec958, dmix,
/// plughw, default, etc.). Only hardware devices (`hw:`) are meaningful for
/// the device picker — they map 1:1 to physical cards.
///
/// On other platforms this function always returns true (no filtering needed).
fn is_hardware_device(id: &str) -> bool {
    // cpal formats device IDs as "host:pcm_id", e.g. "alsa:hw:CARD=Gen,DEV=0".
    // On Linux/ALSA, only keep hw: entries — direct hardware, one per physical
    // card/device. Skips plughw, default, surround51, iec958, dmix, etc.
    //
    // cpal enumerates each card twice: once via HintIter (named form:
    // hw:CARD=Gen,DEV=0) and once via hardware scan (numeric form:
    // hw:CARD=1,DEV=0). The two forms may have slightly different device names
    // ("Scarlett 2i2 4th Gen, USB Audio" vs "Scarlett 2i2 4th Gen"), defeating
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
        // ("Scarlett 2i2 4th Gen, USB Audio" vs "Scarlett 2i2 4th Gen"), defeating
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

pub fn list_input_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    log::trace!("listing input device descriptors");

    // On Linux with the jack feature, JACK is the only supported audio backend.
    // Always enumerate via the jack crate directly — CPAL's JACK host crashes
    // the JACK server, and falling back to ALSA can hang indefinitely on a
    // broken USB device (xHCI reset state). Fail fast if JACK is not running.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if !jack_server_is_running() {
            // JACK died (USB disconnect, restart cycle). Flush stale metadata
            // so the next successful enumeration gets fresh port counts.
            invalidate_jack_meta_cache();
            bail!("JACK server is not running — start jackd before enumerating input devices");
        }
        return jack_enumerate_input_devices();
    }

    #[allow(unreachable_code)]
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
        devices.push(AudioDeviceDescriptor { id, name, channels: max_supported_input_channels(&device).unwrap_or(0) });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}

pub fn list_output_device_descriptors() -> Result<Vec<AudioDeviceDescriptor>> {
    log::trace!("listing output device descriptors");

    // On Linux with the jack feature, JACK is the only supported audio backend.
    // See list_input_device_descriptors() for the rationale — falling back to
    // ALSA can hang indefinitely when the USB device is in a broken kernel state.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if !jack_server_is_running() {
            invalidate_jack_meta_cache();
            bail!("JACK server is not running — start jackd before enumerating output devices");
        }
        return jack_enumerate_output_devices();
    }

    #[allow(unreachable_code)]
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
        devices.push(AudioDeviceDescriptor { id, name, channels: max_supported_output_channels(&device).unwrap_or(0) });
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}
pub fn build_streams_for_project(
    project: &Project,
    runtime_graph: &RuntimeGraph,
) -> Result<Vec<Stream>> {
    log::info!("building audio streams for project");

    // When using direct JACK, no CPAL streams needed — streaming is handled
    // entirely by the jack crate in build_active_chain_runtime.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    if using_jack_direct() {
        log::info!("JACK direct mode: no CPAL streams to build");
        return Ok(Vec::new());
    }

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

/// Build a synthetic ResolvedChainAudioConfig using only the jack crate.
/// No CPAL or ALSA access. The resolved config is only used to provide
/// sample_rate and stream_signature to the runtime graph — the direct JACK
/// backend ignores inputs/outputs entirely.
#[cfg(all(target_os = "linux", feature = "jack"))]
fn jack_resolve_chain_config(chain: &Chain) -> Result<ResolvedChainAudioConfig> {
    // Uses cached metadata — never opens a JACK client on its own. This is
    // called on every upsert_chain/sync_project, so opening a client here
    // used to dominate the client-churn load on jackd.
    let meta = jack_meta()?;
    let sample_rate = meta.sample_rate as f32;
    let in_channels = meta.capture_port_count as u16;
    let out_channels = meta.playback_port_count as u16;

    // Build a minimal stream signature so the runtime can detect config changes
    let input_sigs: Vec<InputStreamSignature> = chain.input_blocks().into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .map(|entry| InputStreamSignature {
            device_id: "jack:system".to_string(),
            channels: entry.channels.clone(),
            stream_channels: in_channels,
            sample_rate: meta.sample_rate,
            buffer_size_frames: meta.buffer_size,
        })
        .collect();

    let output_sigs: Vec<OutputStreamSignature> = chain.output_blocks().into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .map(|entry| OutputStreamSignature {
            device_id: "jack:system".to_string(),
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

        // When using direct JACK, bypass ALL CPAL/ALSA access.
        // Even creating an ALSA host and probing devices can interfere with
        // JACK's exclusive access to the hardware (causing xruns and device loss).
        #[cfg(all(target_os = "linux", feature = "jack"))]
        if using_jack_direct() {
            return self.sync_project_jack_direct(project);
        }

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
        if using_jack_direct() {
            let resolved = jack_resolve_chain_config(chain)?;
            return self.upsert_chain_with_resolved(chain, resolved);
        }

        let host = get_host();
        validate_chain_channels_against_devices(host, chain)?;
        let resolved = resolve_chain_audio_config(host, project, chain)?;
        self.upsert_chain_with_resolved(chain, resolved)
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

    fn upsert_chain_with_resolved(
        &mut self,
        chain: &Chain,
        resolved: ResolvedChainAudioConfig,
    ) -> Result<()> {
        let needs_stream_rebuild = self
            .active_chains
            .get(&chain.id)
            .map(|active| active.stream_signature != resolved.stream_signature)
            .unwrap_or(true);

        let runtime =
            self.runtime_graph
                .upsert_chain(chain, resolved.sample_rate, needs_stream_rebuild)?;

        if needs_stream_rebuild {
            let active = build_active_chain_runtime(&chain.id, chain, resolved, runtime)?;
            self.active_chains.insert(chain.id.clone(), active);
        }

        Ok(())
    }
}

pub fn resolve_project_chain_sample_rates(project: &Project) -> Result<HashMap<ChainId, f32>> {
    // When using direct JACK, get sample rate from JACK server directly.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            // Reuse the cached metadata client — no fresh JACK client here.
            let sr = jack_meta()?.sample_rate as f32;
            let mut sample_rates = HashMap::new();
            for chain in &project.chains {
                if chain.enabled {
                    sample_rates.insert(chain.id.clone(), sr);
                }
            }
            return Ok(sample_rates);
        }
    }

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
fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        name: "Insert Return".to_string(),
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an InsertBlock's send endpoint to an OutputEntry for stream resolution.
fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
    use project::chain::ChainOutputMode;
    OutputEntry {
        name: "Insert Send".to_string(),
        device_id: insert.send.device_id.clone(),
        mode: match insert.send.mode {
            project::chain::ChainInputMode::Mono => ChainOutputMode::Mono,
            _ => ChainOutputMode::Stereo,
        },
        channels: insert.send.channels.clone(),
    }
}

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
fn validate_channels_against_devices(project: &Project, host: &cpal::Host) -> Result<()> {
    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }
        validate_chain_channels_against_devices(host, chain)?;
    }
    Ok(())
}

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

fn validate_input_channels_against_device(
    host: &cpal::Host,
    chain_id: &str,
    device_id: &str,
    channels: &[usize],
) -> Result<()> {
    // When using direct JACK backend, skip CPAL-based channel validation.
    // JACK manages port routing and will report errors at connect time.
    if using_jack_direct() {
        return Ok(());
    }
    let device = find_input_device_by_id(host, device_id)?.ok_or_else(|| {
        anyhow!("chain '{}' missing input device '{}'", chain_id, device_id)
    })?;
    let total_channels = max_supported_input_channels(&device).with_context(|| {
        format!(
            "failed to resolve input channel capacity for '{}'",
            device_id
        )
    })?;
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

fn validate_output_channels_against_device(
    host: &cpal::Host,
    chain_id: &str,
    device_id: &str,
    channels: &[usize],
) -> Result<()> {
    if using_jack_direct() {
        return Ok(());
    }
    let device = find_output_device_by_id(host, device_id)?.ok_or_else(|| {
        anyhow!("chain '{}' missing output device '{}'", chain_id, device_id)
    })?;
    let total_channels = max_supported_output_channels(&device).with_context(|| {
        format!(
            "failed to resolve output channel capacity for '{}'",
            device_id
        )
    })?;
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
fn find_input_device_by_id(host: &cpal::Host, device_id: &str) -> Result<Option<cpal::Device>> {
    for device in host.input_devices()? {
        if device.id()?.to_string() == device_id {
            return Ok(Some(device));
        }
    }
    Ok(None)
}
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
            let jack_client = build_jack_direct_chain(chain_id, chain, runtime)?;
            return Ok(ActiveChainRuntime {
                stream_signature,
                _input_streams: Vec::new(),
                _output_streams: Vec::new(),
                _jack_client: Some(jack_client),
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
    })
}

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

fn required_channel_count(channels: &[usize]) -> usize {
    channels
        .iter()
        .copied()
        .max()
        .map(|channel| channel + 1)
        .unwrap_or(0)
}

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

fn max_supported_input_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = device
        .supported_input_configs()?
        .map(|config| config.channels() as usize)
        .max();
    let default_channels = device
        .default_input_config()
        .ok()
        .map(|config| config.channels() as usize);
    max_supported_channels(default_channels, max_supported)
}

fn max_supported_output_channels(device: &cpal::Device) -> Result<usize> {
    let max_supported = device
        .supported_output_configs()?
        .map(|config| config.channels() as usize)
        .max();
    let default_channels = device
        .default_output_config()
        .ok()
        .map(|config| config.channels() as usize);
    max_supported_channels(default_channels, max_supported)
}

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
    use super::{
        max_supported_channels, resolve_chain_runtime_sample_rate, select_supported_stream_config,
    };
    use cpal::{SampleFormat, SupportedBufferSize, SupportedStreamConfigRange};

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

    #[test]
    fn resolve_chain_runtime_sample_rate_rejects_mismatched_input_and_output_sample_rates() {
        let input = supported_range(2, 48_000, 48_000).with_max_sample_rate();
        let output = supported_range(2, 44_100, 44_100).with_max_sample_rate();

        let error = resolve_chain_runtime_sample_rate("chain:0", &input, &output)
            .expect_err("mismatched rates should fail");

        assert!(error.to_string().contains("sample_rate"));
    }

    #[test]
    fn max_supported_channels_prefers_supported_capacity_over_default() {
        let resolved =
            max_supported_channels(Some(2), Some(8)).expect("supported channels should resolve");

        assert_eq!(resolved, 8);
    }

    #[test]
    fn max_supported_channels_uses_default_when_supported_list_is_empty() {
        let resolved =
            max_supported_channels(Some(2), None).expect("default channels should resolve");

        assert_eq!(resolved, 2);
    }
}
