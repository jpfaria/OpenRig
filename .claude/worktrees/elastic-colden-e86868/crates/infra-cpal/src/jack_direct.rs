//! `build_jack_direct_chain` — assemble the live JACK `AsyncClient` plus
//! its DSP worker thread for one chain on Linux+JACK.
//!
//! Consumes the chain's I/O blocks to:
//!
//! 1. Pick a target jackd server name (input device_id `jack:<name>`,
//!    `hw:<N>` lookup against the USB cards, or first running named
//!    server fallback).
//! 2. Open a `jack::Client` against that server with retry-200ms × 5 to
//!    ride out the libjack "shm not yet up" race documented in #294 /
//!    #308.
//! 3. Register one input port per max(device_in_ch, chain's selected
//!    channels) and one output port per max(device_out_ch, …) so the
//!    AsyncClient port shape stays stable across channel-toggle edits.
//! 4. Allocate the SPSC ring + scratch buffers at MAX_JACK_FRAMES so a
//!    later `jack_set_buffer_size` cannot trigger a realloc on the audio
//!    thread.
//! 5. Spawn the DSP worker thread, pin it to big cores, and hand it the
//!    same ring + wake pair the JackProcessHandler will use.
//! 6. Activate the AsyncClient and connect its ports to system:capture /
//!    system:playback.
//!
//! Setup-time only — every audio-thread helper this fn calls
//! (`SpscRingBuffer::try_read`, `pin_thread_to_cpus`, `detect_big_cores`)
//! is `#[inline]`.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::Arc;

use anyhow::{anyhow, Result};

use domain::ids::ChainId;
use engine::runtime::{process_input_f32, ChainRuntimeState};
use project::block::{AudioBlockKind, InputEntry, OutputEntry};
use project::chain::Chain;

use crate::active_runtime::DspWorkerHandle;
use crate::cpu_affinity::{detect_big_cores, pin_thread_to_cpus};
use crate::jack_handlers::{JackProcessHandler, JackShutdownHandler, SpscRingBuffer};
use crate::jack_supervisor;
use crate::resolved::MAX_JACK_FRAMES;
use crate::usb_proc::{detect_all_usb_audio_cards, jack_server_is_running_for};

pub(crate) fn build_jack_direct_chain(
    chain_id: &ChainId,
    chain: &Chain,
    runtime: Arc<ChainRuntimeState>,
) -> Result<(
    jack::AsyncClient<JackShutdownHandler, JackProcessHandler>,
    DspWorkerHandle,
)> {
    // Determine which named JACK server this chain should connect to.
    let cards = detect_all_usb_audio_cards();
    let server_name = chain
        .input_blocks()
        .into_iter()
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
            cards
                .iter()
                .find(|c| jack_server_is_running_for(&c.server_name))
                .map(|c| c.server_name.clone())
        })
        .unwrap_or_else(|| "default".to_string());

    log::info!(
        "build_jack_direct_chain: chain '{}' → JACK server '{}'",
        chain_id.0,
        server_name
    );

    let client_name = format!("openrig_{}", chain_id.0);
    // Retry up to 5 times with 200ms between attempts.
    // The JACK UNIX socket appears before the shm segments are fully initialized,
    // so the first connection attempt can fail with "Cannot open shm segment".
    let result =
        (|| {
            for attempt in 0..5u32 {
                let _lock = jack_supervisor::live_backend::JACK_DEFAULT_SERVER_LOCK
                    .lock()
                    .unwrap();
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
    let (client, _status) = result.map_err(|e| {
        anyhow!(
            "failed to create JACK client for server '{}': {:?}",
            server_name,
            e
        )
    })?;

    let sample_rate = client.sample_rate() as f32;
    let buf_size = client.buffer_size() as usize;
    log::info!(
        "JACK direct: client '{}', sample_rate={}, buffer_size={}",
        client_name,
        sample_rate,
        buf_size
    );

    // Collect chain's configured input/output entries — used only to size the
    // interleave scratch for the `max_in_ch / max_out_ch` picked below.
    let input_entries: Vec<&InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter())
        .collect();

    // Register every port the physical device exposes, not just the channels
    // the chain currently consumes. This keeps the AsyncClient stable across
    // chain edits that toggle channel selection (mono[0] ↔ mono[1] ↔ stereo):
    // port count never changes, so `upsert_chain_with_resolved` can keep the
    // same client alive and sidestep the libjack state-corruption regression
    // documented in issue #294 / #308 bug 1 ("Cannot open shm segment" after
    // rebuild).
    //
    // Fall back to the chain's own max when the card cannot be looked up so
    // degraded deployments (missing /proc/asound entry) still work.
    let device_in_ch = cards
        .iter()
        .find(|c| c.server_name == server_name)
        .map(|c| c.capture_channels as usize)
        .unwrap_or(0);
    let chain_max_in = input_entries
        .iter()
        .flat_map(|e| e.channels.iter())
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(1);
    let max_in_ch = device_in_ch.max(chain_max_in);

    // Collect output channel requirements from chain
    let output_entries: Vec<&OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter())
        .collect();

    let device_out_ch = cards
        .iter()
        .find(|c| c.server_name == server_name)
        .map(|c| c.playback_channels as usize)
        .unwrap_or(0);
    let chain_max_out = output_entries
        .iter()
        .flat_map(|e| e.channels.iter())
        .copied()
        .max()
        .map(|m| m + 1)
        .unwrap_or(2);
    let max_out_ch = device_out_ch.max(chain_max_out);

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

    // Set up DSP worker thread with ring buffer. Size the slot for the
    // largest buffer the UI can pick (MAX_JACK_FRAMES = 4096) × port count,
    // so a live `jack_set_buffer_size` that grows `n_frames` at runtime
    // never triggers a realloc in the RT callback. Memory cost is bounded
    // (4096 × max_in_ch × 4 bytes × 8 slots ≈ 1 MB worst-case).
    let samples_per_buffer = MAX_JACK_FRAMES * max_in_ch;
    // 8 slots: enough headroom for JACK to write while worker processes
    let ring = Arc::new(SpscRingBuffer::new(8, samples_per_buffer));
    let wake = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Seed with current buffer size so the worker's first iteration slices
    // read_buf correctly even if no callback has fired yet.
    let current_n_frames = Arc::new(std::sync::atomic::AtomicUsize::new(buf_size));

    let handler = JackProcessHandler {
        // Scratch buffers pre-sized for MAX_JACK_FRAMES so
        // JackProcessHandler::process never reallocates when jackd raises
        // the per-callback `n_frames` via jack_set_buffer_size.
        input_buf: vec![0.0f32; MAX_JACK_FRAMES * input_ports.len().max(1)],
        output_buf: vec![0.0f32; MAX_JACK_FRAMES * output_ports.len().max(1)],
        input_ports,
        output_ports,
        runtime: Arc::clone(&runtime),
        input_ring: Some(Arc::clone(&ring)),
        worker_wake: Some(Arc::clone(&wake)),
        current_n_frames: Arc::clone(&current_n_frames),
        affinity_pinned: false,
    };

    // Spawn DSP worker thread
    let worker_runtime = Arc::clone(&runtime);
    let worker_ring = Arc::clone(&ring);
    let worker_wake = Arc::clone(&wake);
    let worker_stop = Arc::clone(&stop_flag);
    let worker_channels = max_in_ch;
    let worker_chain_id = chain_id.0.clone();
    let worker_current_frames = Arc::clone(&current_n_frames);
    let thread = std::thread::Builder::new()
        .name(format!("dsp-worker-{}", chain_id.0))
        .spawn(move || {
            // Pin to big cores (A76 on RK3588)
            let big_cores = detect_big_cores();
            if !big_cores.is_empty() {
                pin_thread_to_cpus(&big_cores);
                log::info!(
                    "DSP worker '{}': pinned to cores {:?}",
                    worker_chain_id,
                    big_cores
                );
            }

            // Set high priority (not RT, but high normal)
            unsafe {
                let param = libc::sched_param { sched_priority: 0 };
                libc::sched_setscheduler(0, libc::SCHED_OTHER, &param);
                // Use nice -10 for higher scheduling priority
                libc::setpriority(libc::PRIO_PROCESS, 0, -10);
            }

            let mut read_buf = vec![0.0f32; samples_per_buffer];
            log::info!(
                "DSP worker '{}': started (buf_size={}, channels={})",
                worker_chain_id,
                buf_size,
                worker_channels
            );

            loop {
                if worker_stop.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }

                // Process all available buffers. Slice `read_buf` to the
                // ACTUAL n_frames jackd is currently delivering so the
                // engine never processes ring-buffer padding (which would
                // add latency equal to padding/sample_rate and burn CPU on
                // silence).
                let mut processed_any = false;
                while worker_ring.try_read(&mut read_buf) {
                    let n_frames = worker_current_frames
                        .load(std::sync::atomic::Ordering::Relaxed)
                        .min(MAX_JACK_FRAMES)
                        .max(1);
                    let needed = (n_frames * worker_channels).min(read_buf.len());
                    let real = &read_buf[..needed];
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_f32(&worker_runtime, 0, real, worker_channels);
                    }));
                    processed_any = true;
                }

                if !processed_any {
                    // Wait for wake signal. Must check the flag before
                    // waiting: Condvar::notify_one() with no waiter is LOST
                    // (POSIX semantics), so if jackd wrote to the ring AND
                    // notified between `try_read` returning empty and here,
                    // blocking unconditionally would miss the wake and stall
                    // the worker for the full timeout. Consuming the flag
                    // after the wait also closes the race window going
                    // forward.
                    //
                    // Timeout is kept short (2ms) as a safety net so the stop
                    // flag is still polled quickly on shutdown. At 64 samples
                    // @ 48 kHz the audio period is 1.33 ms; any wait longer
                    // than ~1 period risks swallowing multiple buffers on a
                    // missed notification and producing an audible click.
                    let mut flag = worker_wake.0.lock().unwrap();
                    if !*flag {
                        let (new_flag, _) = worker_wake
                            .1
                            .wait_timeout(flag, std::time::Duration::from_millis(2))
                            .unwrap();
                        flag = new_flag;
                    }
                    *flag = false;
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
    let active_client = client
        .activate_async(notification_handler, handler)
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
        chain_id.0,
        max_in_ch,
        max_out_ch
    );

    Ok((active_client, worker_handle))
}
