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
/// PREEMPTIBLE, with an explicit computation budget. An unpromoted busy
/// thread is demoted to E-cores by macOS (measured: 167 xruns/256
/// underruns in the 60 s real-streams test); an RT thread that overruns a
/// too-small budget is demoted too (the reverted #670 promotion).
///
/// Issue #698: the budget must reflect the chain's REAL cost, not a fixed
/// fraction. Five chains each declaring 85% of the period overcommit the
/// time-constraint band and the kernel demotes workers — measured headless
/// as 61 underruns/20 s with the owner's five-chain project, while the
/// same chains ran clean solo and dual. The worker starts at 85% (a cold
/// chain's cost is unknown and an undersized budget also demotes — the
/// reverted #670 attempt) and then re-declares from its own measured cost
/// (see `BudgetTracker`), so concurrent chains fit the band together.
#[cfg(target_os = "macos")]
fn promote_to_audio_rt(period_ns: u64, computation_ns: u64) {
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
            computation: to_mach(computation_ns.min(period_ns * 85 / 100)),
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
fn promote_to_audio_rt(_period_ns: u64, _computation_ns: u64) {}

/// Issue #698 — adaptive RT computation budget. Buffers measured per
/// window; at each window boundary the worker re-declares its
/// time-constraint computation from the measured worst case plus
/// headroom, so N concurrent chains together stay inside what the
/// kernel's admission will schedule. Plain data, worker-thread only.
struct BudgetTracker {
    window_max_ns: u64,
    window_count: u32,
    declared_ns: u64,
}

impl BudgetTracker {
    /// ≈3 s of buffers at 64 frames / 44.1 kHz between re-declarations.
    const WINDOW: u32 = 2048;

    fn new(declared_ns: u64) -> Self {
        Self {
            window_max_ns: 0,
            window_count: 0,
            declared_ns,
        }
    }

    /// Record one processed buffer's cost; returns the new computation
    /// budget when the window closes AND it differs meaningfully from the
    /// declared one (hysteresis: 10% of the period).
    ///
    /// Fast up-correction: a single buffer OVER the declared budget means
    /// the chain just got heavier (live rebuild, block added) and the
    /// kernel will demote an over-budget RT thread — re-declare the
    /// conservative 85% immediately instead of waiting for the window
    /// (measured: 128 underruns in the post-rebuild stretch of
    /// `rebuild_while_playing_keeps_the_cushion` without this).
    fn observe(&mut self, elapsed_ns: u64, period_ns: u64) -> Option<u64> {
        if elapsed_ns > self.declared_ns && self.declared_ns < period_ns * 85 / 100 {
            return Some(self.reset(period_ns));
        }
        self.window_max_ns = self.window_max_ns.max(elapsed_ns);
        self.window_count += 1;
        if self.window_count < Self::WINDOW {
            return None;
        }
        // Measured worst case + 25% headroom, floored at 10% of the
        // period (an undersized budget also demotes — the reverted #670
        // attempt) and capped at the validated 85%.
        let target = (self.window_max_ns + self.window_max_ns / 4)
            .clamp(period_ns / 10, period_ns * 85 / 100);
        self.window_max_ns = 0;
        self.window_count = 0;
        if target.abs_diff(self.declared_ns) > period_ns / 10 {
            self.declared_ns = target;
            return Some(target);
        }
        None
    }

    /// After a saturation spiral the chain's cost is unknown again —
    /// restart from the conservative cold-start budget.
    fn reset(&mut self, period_ns: u64) -> u64 {
        self.window_max_ns = 0;
        self.window_count = 0;
        self.declared_ns = period_ns * 85 / 100;
        self.declared_ns
    }
}

/// Slots in the ring. 16 buffers ≈ 21 ms at 64 frames — far beyond any
/// transient worker stall that wouldn't already be audible.
const RING_SLOTS: usize = 16;

/// Saturation-recovery policy (issue #670). Owner-hit failure mode: a one-off
/// multi-ms stall builds backlog; the worker then runs chronically over its
/// declared RT computation budget, the kernel demotes it (to an E core), and
/// EVERY buffer becomes multi-ms — the ring pins at its overflow clamp and the
/// chain never heals. The policy: after `threshold` CONSECUTIVE saturated
/// drains, demand recovery — the worker re-asserts its realtime promotion and
/// drops the backlog to bound latency. A single healthy drain resets the run.
pub(crate) struct SaturationRecovery {
    threshold: u32,
    run: u32,
}

impl SaturationRecovery {
    pub(crate) fn new(threshold: u32) -> Self {
        Self { threshold, run: 0 }
    }

    /// Record one drain; `saturated` = the backlog hit the overflow clamp.
    /// Returns `true` when recovery must run NOW (and restarts the counter).
    pub(crate) fn observe(&mut self, saturated: bool) -> bool {
        if !saturated {
            self.run = 0;
            return false;
        }
        self.run += 1;
        if self.run >= self.threshold {
            self.run = 0;
            return true;
        }
        false
    }
}

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
            let rt_period_ns = period_ns.max(500_000);
            // Cold start: the chain's cost is unknown, declare the
            // validated 85% (#670); the BudgetTracker then re-declares
            // from measured cost so concurrent chains fit the RT band
            // together (#698).
            let mut budget = BudgetTracker::new(rt_period_ns * 85 / 100);
            promote_to_audio_rt(rt_period_ns, budget.declared_ns);
            // Measured on the real-streams test: joining the device workgroup
            // and/or spinning the idle gap made the tail WORSE and erratic
            // (50/15/2 xruns per 60 s vs 0-11 without). Plain preemptible RT
            // with short idle sleeps is the best-behaving configuration; the
            // residual tail is diagnosed via the log below.
            let mut local = vec![0.0_f32; max_buffer_samples];
            let spin_budget = std::time::Duration::from_nanos(period_ns * 35 / 100);
            let mut idle_since: Option<std::time::Instant> = None;
            // ~43 ms of pinned backlog at 64 frames before declaring the
            // death spiral and recovering.
            let mut recovery = SaturationRecovery::new(32);
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
                let saturated = w - r > RING_SLOTS - 2;
                if saturated {
                    r = w - (RING_SLOTS - 2);
                }
                if recovery.observe(saturated) {
                    // Death spiral detected: the kernel has likely demoted
                    // this thread after the sustained over-budget churn.
                    // Re-assert the realtime promotion and drop the backlog
                    // to ONE buffer so latency is bounded again. Worker
                    // thread, rare event — the log is allowed.
                    promote_to_audio_rt(rt_period_ns, budget.reset(rt_period_ns));
                    r = w.saturating_sub(1);
                    log::warn!(
                        "[#670 worker] saturation spiral: re-promoted realtime and dropped backlog"
                    );
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
                // #698: re-declare the RT computation budget from measured
                // cost at window boundaries so N concurrent workers fit the
                // kernel's time-constraint admission together. Rare (≥3 s
                // apart, only on meaningful change), between buffers.
                if let Some(comp_ns) = budget.observe(elapsed.as_nanos() as u64, rt_period_ns) {
                    promote_to_audio_rt(rt_period_ns, comp_ns);
                }
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
