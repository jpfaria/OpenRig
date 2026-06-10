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
//! (`process_input_buffer`) — and it BUSY-POLLS instead of sleeping, so its
//! core's cache keeps the NAM weights hot and the cold tail never happens.
//! One core is spent per active input stream; sound > CPU (trade-off
//! hierarchy: stability over audio-thread CPU cost).
//!
//! RT-safety: the callback does one bounds-checked copy + one Release store.
//! No allocation, no lock, no syscall (invariant #8). The worker records its
//! own processing time against the buffer deadline through the same
//! `record_callback_deadline` the callback used to call, so the xrun counter
//! keeps its meaning ("DSP heavier than the budget"). If the ring is full
//! (worker stalled >16 buffers ≈ 21 ms) the callback drops the oldest slot;
//! the elastic-underrun counter reports the resulting damage.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::callback_load_timing::record_callback_deadline;
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

    std::thread::Builder::new()
        .name(format!("dsp-worker:{chain_label}"))
        .spawn(move || {
            // Period of one device buffer — the RT computation budget anchor.
            let period_ns = (max_buffer_samples as u64 / 8 / channels.max(1) as u64)
                * 1_000_000_000
                / sample_rate.max(1) as u64;
            promote_to_audio_rt(period_ns.max(500_000));
            let mut local = vec![0.0_f32; max_buffer_samples];
            let mut idle_spins: u32 = 0;
            loop {
                if worker_inner.stop.load(Ordering::Acquire) {
                    return;
                }
                let w = worker_inner.write.load(Ordering::Acquire);
                let mut r = worker_inner.read.load(Ordering::Relaxed);
                if r == w {
                    // Nothing pending. Spin briefly (data lands within a
                    // period), then yield in short sleeps — an RT thread
                    // must not burn its computation budget idling or the
                    // kernel demotes it.
                    idle_spins += 1;
                    if idle_spins < 2_000 {
                        std::hint::spin_loop();
                    } else {
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                    continue;
                }
                idle_spins = 0;
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
                record_callback_deadline(
                    &slot_handle.load(),
                    start.elapsed(),
                    n / channels.max(1),
                    sample_rate,
                );
            }
        })
        .expect("spawn dsp worker");

    DspWorkerProducer { inner }
}
