//! Audio-thread JACK handlers + lock-free producer/consumer ring used
//! between the JACK RT callback and the DSP worker.
//!
//! Everything here is on the JACK real-time path:
//!
//! - `SpscRingBuffer` — single-producer / single-consumer ring of fixed
//!   slots. The RT callback writes interleaved f32 blocks; the DSP
//!   worker reads them. Power-of-two slot count for free modulo.
//! - `JackShutdownHandler` — replaces libjack's default `()` notification
//!   handler so a USB unplug / jackd crash sets an atomic flag instead
//!   of calling `std::process::exit(0)`.
//! - `JackProcessHandler` — the RT process callback itself. Pins itself
//!   to big cores on first invocation, copies port data in/out, hands
//!   the input block to either the worker (via the ring) or the engine
//!   (inline fallback when no worker is configured).
//!
//! Inlining contract (issue #194 Phase 5): every helper that
//! `JackProcessHandler::process` reaches across this module boundary —
//! `SpscRingBuffer::try_write`, `pin_thread_to_cpus`, `detect_big_cores`
//! — is `#[inline]`. The audio thread cannot pay an extra call/jump
//! because of refactor.

#![cfg(all(target_os = "linux", feature = "jack"))]

use std::sync::Arc;

use engine::runtime::{process_input_f32, process_output_f32, ChainRuntimeState};

use crate::cpu_affinity::{detect_big_cores, pin_thread_to_cpus};

/// Lock-free single-producer single-consumer ring buffer for passing audio
/// data from the JACK RT callback to the DSP worker thread.
///
/// The JACK callback writes interleaved f32 blocks; the worker reads them.
/// Slots are fixed-size (max_samples_per_slot), indexed by atomic counters.
pub(crate) struct SpscRingBuffer {
    /// Flat storage: `num_slots * max_samples_per_slot` f32s.
    data: Vec<std::cell::UnsafeCell<f32>>,
    /// How many f32 samples each slot holds.
    pub(crate) max_samples_per_slot: usize,
    /// Number of slots (power of 2 for fast modulo).
    num_slots: usize,
    /// Monotonically increasing write counter (slot index = write_pos % num_slots).
    write_pos: std::sync::atomic::AtomicUsize,
    /// Monotonically increasing read counter.
    read_pos: std::sync::atomic::AtomicUsize,
}

unsafe impl Send for SpscRingBuffer {}
unsafe impl Sync for SpscRingBuffer {}

impl SpscRingBuffer {
    pub(crate) fn new(num_slots: usize, max_samples_per_slot: usize) -> Self {
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
    #[inline]
    pub(crate) fn try_write(&self, samples: &[f32]) -> bool {
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
    #[inline]
    pub(crate) fn try_read(&self, dst: &mut [f32]) -> bool {
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
pub(crate) struct JackShutdownHandler {
    pub(crate) shutdown_flag: Arc<std::sync::atomic::AtomicBool>,
}

impl jack::NotificationHandler for JackShutdownHandler {
    unsafe fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        log::warn!("JACK server shutdown: {:?} — {}", status, reason);
        self.shutdown_flag.store(true, std::sync::atomic::Ordering::Release);
        // The supervisor's health_check will probe the server on the next
        // health tick and classify it as Zombie; that triggers try_reconnect
        // which calls supervisor.shutdown_all + fresh ensure_server. No
        // global cache to invalidate here.
        // Do NOT call std::process::exit() — let the health timer handle it.
    }
}

/// Direct JACK process handler — runs in the JACK real-time thread.
/// Does NO DSP processing — only copies audio data to/from ring buffers.
/// The heavy DSP work happens in a separate worker thread.
///
/// Buffers are pre-allocated to avoid heap allocation in the RT callback.
pub(crate) struct JackProcessHandler {
    pub(crate) input_ports: Vec<jack::Port<jack::AudioIn>>,
    pub(crate) output_ports: Vec<jack::Port<jack::AudioOut>>,
    pub(crate) runtime: Arc<ChainRuntimeState>,
    pub(crate) input_buf: Vec<f32>,
    pub(crate) output_buf: Vec<f32>,
    /// Ring buffer for offloading DSP to the worker thread.
    /// When Some, the RT callback writes input to this ring and the worker
    /// thread does the processing. When None, processing is done inline
    /// (fallback for non-Linux or when worker setup fails).
    pub(crate) input_ring: Option<Arc<SpscRingBuffer>>,
    /// Condvar to wake the worker thread when new input is available.
    pub(crate) worker_wake: Option<Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>>,
    /// Current n_frames from the JACK callback. Written by the RT thread
    /// each callback (Relaxed store), read by the DSP worker (Relaxed load)
    /// to know how many samples of `read_buf` are real vs ring padding.
    /// Without this the worker would process `MAX_JACK_FRAMES * channels`
    /// every iteration regardless of jackd's actual buffer size, adding
    /// latency and wasting CPU on zero-padded tail samples.
    pub(crate) current_n_frames: Arc<std::sync::atomic::AtomicUsize>,
    /// `true` once the RT thread has pinned itself to the big cores.
    /// libjack spawns the thread that ends up calling `process` lazily
    /// inside its own infrastructure, and there is no public hook to
    /// configure its affinity at creation. We therefore pin on the first
    /// call from the thread itself — a one-time write, no hot-path cost.
    pub(crate) affinity_pinned: bool,
}

impl jack::ProcessHandler for JackProcessHandler {
    fn process(&mut self, _client: &jack::Client, ps: &jack::ProcessScope) -> jack::Control {
        // libjack creates the RT callback thread inside our process when
        // `Client::activate` runs. The thread inherits the process-wide
        // CPU mask (set by systemd's CPUAffinity=0-3 in the service
        // drop-in), which forces the audio-critical callback onto the
        // little A55 cores where it competes with the Slint UI thread
        // and the Mesa llvmpipe workers. On the first invocation from
        // this thread we widen the mask to the big A76 cores so the
        // callback runs alongside the DSP worker on the isolated RT
        // cores instead. `sched_setaffinity` may widen beyond the
        // service-level mask because systemd uses affinity — not a
        // cgroup cpuset — to apply CPUAffinity=. Check-and-set is
        // racy-safe here: the thread only calls itself.
        if !self.affinity_pinned {
            let big_cores = detect_big_cores();
            if !big_cores.is_empty() {
                pin_thread_to_cpus(&big_cores);
                log::info!(
                    "JackProcessHandler: RT callback thread pinned to big cores {:?}",
                    big_cores
                );
            }
            self.affinity_pinned = true;
        }
        let n_frames = ps.n_frames() as usize;
        // Publish the current callback size so the DSP worker only processes
        // the real samples, not the ring-buffer padding. Relaxed ordering is
        // enough — the wake-notify pair below provides the happens-before
        // relationship; the worker just needs a recent value.
        self.current_n_frames
            .store(n_frames, std::sync::atomic::Ordering::Relaxed);

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
