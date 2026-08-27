//! Responsibility: runs one input chain DSP off the audio callback.
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
//! computation budget, and when the ring is empty it SLEEPS (yields the
//! P-core) instead of busy-spinning: a spin HELD the core and starved the
//! CoreAudio HAL + sibling RT workers + VST3 audio threads, oversubscribing
//! the realtime band on a multi-interface rig and preempting the workers
//! OFF-CPU (#781 — 34x fewer off-cpu late buffers on the real-streams repro).
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

pub(crate) use crate::budget_tracker::BudgetTracker;
use crate::live_runtime::LiveRuntimeSlot;
use crate::process_input_buffer;
/// Slots in the ring. 16 buffers ≈ 21 ms at 64 frames — far beyond any
/// transient worker stall that wouldn't already be audible.
const RING_SLOTS: usize = 16;
pub(crate) use crate::rt_thread_policy::{promote_to_audio_rt, thread_cpu_time_ns};
pub(crate) use crate::saturation_recovery::SaturationRecovery;

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
    device_uid: Option<String>,
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
            // #760: co-schedule this worker with ITS OWN device's IO thread so
            // the kernel keeps it on a P-core under contention (the residual
            // "RT thread still late under load" tail). The earlier "joining the
            // workgroup made it WORSE" result (#670) was measured with the join
            // hard-coded to the SYSTEM DEFAULT device (the #760 bug) — under a
            // multi-device rig the worker joined the wrong device's workgroup
            // and was mis-scheduled. Now that the join resolves the bound
            // device's UID, the worker co-schedules with the device it serves.
            let _workgroup = crate::audio_workgroup::join_input(device_uid.as_deref()); // #779: leave on thread exit
            let mut local = vec![0.0_f32; max_buffer_samples];
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
                    // #781: ring empty — sleep to YIELD the P-core; never busy-spin
                    // the gap. The old 35% spin held the core, starving the CoreAudio
                    // HAL + sibling RT workers + VST3 audio threads -> RT-band
                    // oversubscription on 5 P-cores -> workers preempted OFF-CPU (the
                    // #781 flood). Measured (H4): same latency, ~1/3 the CPU.
                    std::thread::sleep(std::time::Duration::from_micros(100));
                    continue;
                }
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
                        "dsp-worker: saturation spiral — re-promoted realtime and dropped backlog"
                    );
                }
                let slot = &worker_inner.slots[r % RING_SLOTS];
                let n = slot.len.load(Ordering::Relaxed).min(local.len());
                local[..n].copy_from_slice(&slot.data[..n]);
                worker_inner.read.store(r + 1, Ordering::Relaxed);

                // Measure BOTH: thread CPU time (real compute, immune to
                // preemption — drives the RT budget + load meter) and wall-clock
                // (delivery latency — drives the late-buffer diagnostic).
                let cpu0 = thread_cpu_time_ns();
                let start = std::time::Instant::now();
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    process_input_buffer(&slot_handle, input_index, &local[..n], channels);
                }));
                let elapsed = start.elapsed();
                let wall_ns = elapsed.as_nanos() as u64;
                // Compute time = CPU the DSP actually used. If the thread was
                // descheduled mid-DSP, wall_ns balloons but compute_ns does not.
                // Fall back to wall-clock where thread CPU time is unavailable.
                let compute_ns = match (cpu0, thread_cpu_time_ns()) {
                    (Some(a), Some(b)) => b.saturating_sub(a),
                    _ => wall_ns,
                };
                let frames = (n / channels.max(1)) as u64;
                let buf_period_ns = frames * 1_000_000_000 / sample_rate.max(1) as u64;
                // Load meter = real CPU load (compute), not wall-clock. A
                // wall-clock spike is preemption, not load; reporting it as
                // "load" misreads as overload on a machine with headroom.
                slot_handle
                    .load()
                    .record_worker_load(compute_ns, buf_period_ns);
                // #698: re-declare the RT computation budget from measured COMPUTE
                // cost at window boundaries so N concurrent workers fit the
                // kernel's time-constraint admission together — and a preemption
                // stall (wall-clock) never churns the policy. Rare, between buffers.
                if let Some(comp_ns) = budget.observe(compute_ns, rt_period_ns) {
                    promote_to_audio_rt(rt_period_ns, comp_ns);
                }
                // #670 diagnostic: name the magnitude of a late buffer so a
                // ~1.4 ms cold-compute tail is distinguishable from a multi-ms
                // preemption. Worker thread (not the HAL callback); fires only
                // on the rare late buffer.
                if elapsed.as_nanos() as u64 > buf_period_ns {
                    log::trace!(
                        "dsp-worker: late buffer: {}us wall / {}us cpu (period {}us, backlog {})",
                        elapsed.as_micros(),
                        compute_ns / 1000,
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

#[cfg(test)]
#[path = "dsp_worker_tests.rs"]
mod tests;
