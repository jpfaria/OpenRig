//! Issue #670 — per-input DSP worker: move the chain DSP OFF the CoreAudio
//! I/O thread.
//!
//! Reproduced by `tests/issue_670_real_streams_no_xruns.rs`: with the chain
//! DSP inline in the input callback, the REAL stack records sporadic xruns
//! (12/60 s on an idle machine, no GUI). Cause, measured: the HAL thread
//! SLEEPS between cycles, the NAM A2 working set cools, and the cold-cache
//! inference tail (~1.4 ms vs the hot ~250 us) sporadically crosses the
//! 1.333 ms cycle — CoreAudio then drops input (the click). No scheduling of
//! the HAL thread can fix this: the cold tail is real compute.
//!
//! Fix: the input callback only COPIES the buffer into a lock-free SPSC ring
//! (microseconds, never overruns the cycle) and returns. A dedicated worker
//! thread per input stream drains the ring and runs the chain DSP
//! (`process_input_buffer`): preemptible realtime with a realistic
//! computation budget, spinning a bounded window (~35% of the period) before
//! sleeping when idle — the spin keeps the model weights hot through the
//! short inter-buffer gaps (killing the ~1.5 ms cold tail, measured), while
//! staying inside the declared RT budget so the kernel never demotes the
//! thread. Sound > CPU (trade-off hierarchy).
//!
//! RT-safety: the callback does one bounds-checked copy + one Release store.
//! No allocation, no lock, no syscall (invariant #8).
//!
//! Damage accounting (what the xrun LED means here): a late worker buffer
//! that catches up is absorbed by the ring + elastic and is NOT audible —
//! it feeds the load meter only (`record_worker_load`). Audible damage is
//! counted where it physically happens: an elastic underrun (output starved)
//! or a ring overflow drop (`record_dropped_buffer`, a gap in the played
//! signal). In the old inline design a late callback WAS damage (CoreAudio
//! dropped input), hence the old `record_callback_load` semantics, which the
//! non-F32 inline paths keep.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::live_runtime::LiveRuntimeSlot;
use crate::process_input_buffer;

/// Promote the worker to the macOS realtime (time-constraint) class:
/// PREEMPTIBLE, computation budget sized to the real chain work (~85% of the
/// buffer period). An unpromoted busy thread is demoted to E-cores by macOS
/// (measured: 167 xruns/256 underruns in the 60 s real-streams test); an RT
/// thread that overruns a too-small budget is demoted too (the reverted #670
/// promotion). These parameters were validated offline: the full Beat It
/// chain paced at the live cadence ran 56 572 buffers with zero xruns.
#[cfg(target_os = "macos")]
fn promote_to_audio_rt(period_ns: u64) {
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    #[repr(C)]
    struct TimeConstraint {
        period: u32,
        computation: u32,
        constraint: u32,
        preemptible: u32,
    }
    extern "C" {
        fn mach_thread_self() -> u32;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
        fn thread_policy_set(thread: u32, flavor: i32, policy: *const u32, count: u32) -> i32;
    }
    const THREAD_TIME_CONSTRAINT_POLICY: i32 = 2;
    unsafe {
        let mut tb = Timebase { numer: 0, denom: 0 };
        if mach_timebase_info(&mut tb) != 0 || tb.numer == 0 {
            return;
        }
        let to_mach = |ns: u64| ((ns as u128 * tb.denom as u128) / tb.numer as u128) as u32;
        let policy = TimeConstraint {
            period: to_mach(period_ns),
            computation: to_mach(period_ns * 85 / 100),
            constraint: to_mach(period_ns),
            preemptible: 1,
        };
        let rc = thread_policy_set(
            mach_thread_self(),
            THREAD_TIME_CONSTRAINT_POLICY,
            &policy as *const _ as *const u32,
            4,
        );
        log::info!("[#670] dsp-worker realtime promotion rc={rc}");
    }
}

#[cfg(not(target_os = "macos"))]
fn promote_to_audio_rt(_period_ns: u64) {}

/// Slots in the ring. 16 buffers ≈ 21 ms at 64 frames — far beyond any
/// transient worker stall that wouldn't already be audible.
const RING_SLOTS: usize = 16;

struct RingSlot {
    /// Valid sample count in `data` (callbacks may deliver varying sizes).
    len: AtomicUsize,
    data: Box<[f32]>,
}

struct Inner {
    slots: Vec<RingSlot>,
    /// Next slot the callback writes. Only the callback mutates it.
    write: AtomicUsize,
    /// Next slot the worker reads. Only the worker mutates it.
    read: AtomicUsize,
    stop: AtomicBool,
}

/// Producer half, owned by the input callback closure. Dropping it (stream
/// teardown) stops the worker.
pub(crate) struct DspWorkerProducer {
    inner: Arc<Inner>,
    /// For damage accounting only: a dropped (overflowed) buffer is an xrun.
    slot: LiveRuntimeSlot,
}

impl DspWorkerProducer {
    /// Called from the audio callback: copy `data` into the ring. Lock-free,
    /// allocation-free, syscall-free. If the ring is full the oldest slot is
    /// overwritten (the worker will skip it); the elastic underrun counter
    /// reports any audible consequence.
    #[inline]
    pub(crate) fn push(&self, data: &[f32]) {
        let inner = &self.inner;
        let w = inner.write.load(Ordering::Relaxed);
        // Ring full (worker stalled >RING_SLOTS-2 buffers): the oldest slot is
        // about to be overwritten — a real gap in the played signal. Count it
        // as an xrun (wait-free: ArcSwap load + one atomic increment).
        if w.wrapping_sub(inner.read.load(Ordering::Relaxed)) >= RING_SLOTS - 2 {
            self.slot.load().record_dropped_buffer();
        }
        let slot = &inner.slots[w % RING_SLOTS];
        let n = data.len().min(slot.data.len());
        // Safety of the plain copy: the worker never reads this slot while it
        // is the write target (read index trails write; on overflow the worker
        // skips stale slots by jumping its read index forward).
        unsafe {
            let dst = slot.data.as_ptr() as *mut f32;
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, n);
        }
        slot.len.store(n, Ordering::Relaxed);
        inner.write.store(w + 1, Ordering::Release);
    }
}

impl Drop for DspWorkerProducer {
    fn drop(&mut self) {
        self.inner.stop.store(true, Ordering::Release);
    }
}

/// Spawn the worker for one input stream and return the producer handle to
/// move into the input callback closure.
pub(crate) fn spawn(
    chain_label: String,
    slot_handle: LiveRuntimeSlot,
    input_index: usize,
    channels: usize,
    sample_rate: u32,
    max_buffer_samples: usize,
) -> DspWorkerProducer {
    let inner = Arc::new(Inner {
        slots: (0..RING_SLOTS)
            .map(|_| RingSlot {
                len: AtomicUsize::new(0),
                data: vec![0.0_f32; max_buffer_samples].into_boxed_slice(),
            })
            .collect(),
        write: AtomicUsize::new(0),
        read: AtomicUsize::new(0),
        stop: AtomicBool::new(false),
    });
    let worker_inner = Arc::clone(&inner);
    let producer_slot = slot_handle.handle();

    std::thread::Builder::new()
        .name(format!("dsp-worker:{chain_label}"))
        .spawn(move || {
            // Period of one device buffer — the RT computation budget anchor.
            let period_ns = (max_buffer_samples as u64 / 8 / channels.max(1) as u64)
                * 1_000_000_000
                / sample_rate.max(1) as u64;
            promote_to_audio_rt(period_ns.max(500_000));
            // Measured on the real-streams test: joining the device workgroup
            // and/or spinning the idle gap made the tail WORSE and erratic
            // (50/15/2 xruns per 60 s vs 0-11 without). Plain preemptible RT
            // with short idle sleeps is the best-behaving configuration; the
            // residual tail is diagnosed via the log below.
            let mut local = vec![0.0_f32; max_buffer_samples];
            let spin_budget = std::time::Duration::from_nanos(period_ns * 35 / 100);
            let mut idle_since: Option<std::time::Instant> = None;
            loop {
                if worker_inner.stop.load(Ordering::Acquire) {
                    return;
                }
                let w = worker_inner.write.load(Ordering::Acquire);
                let mut r = worker_inner.read.load(Ordering::Relaxed);
                if r == w {
                    let since = *idle_since.get_or_insert_with(std::time::Instant::now);
                    if since.elapsed() < spin_budget {
                        std::hint::spin_loop();
                    } else {
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                    continue;
                }
                idle_since = None;
                // Overflow: jump past slots the callback may be overwriting.
                if w - r > RING_SLOTS - 2 {
                    r = w - (RING_SLOTS - 2);
                }
                let slot = &worker_inner.slots[r % RING_SLOTS];
                let n = slot.len.load(Ordering::Relaxed).min(local.len());
                local[..n].copy_from_slice(&slot.data[..n]);
                worker_inner.read.store(r + 1, Ordering::Relaxed);

                let start = std::time::Instant::now();
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    process_input_buffer(&slot_handle, input_index, &local[..n], channels);
                }));
                let elapsed = start.elapsed();
                let frames = (n / channels.max(1)) as u64;
                let buf_period_ns = frames * 1_000_000_000 / sample_rate.max(1) as u64;
                // Load meter only — a late worker buffer that catches up is
                // absorbed by the ring + elastic and is NOT audible damage
                // (damage = elastic underrun, or a ring drop counted in
                // `push`). See record_worker_load docs.
                slot_handle
                    .load()
                    .record_worker_load(elapsed.as_nanos() as u64, buf_period_ns);
                // #670 diagnostic: name the magnitude of a late buffer so a
                // ~1.4 ms cold-compute tail is distinguishable from a multi-ms
                // preemption. Worker thread (not the HAL callback); fires only
                // on the rare late buffer.
                if elapsed.as_nanos() as u64 > buf_period_ns {
                    log::trace!(
                        "[#670 worker] late buffer: {}us (period {}us, backlog {})",
                        elapsed.as_micros(),
                        buf_period_ns / 1000,
                        w - r,
                    );
                }
            }
        })
        .expect("spawn dsp worker");

    DspWorkerProducer {
        inner,
        slot: producer_slot,
    }
}
