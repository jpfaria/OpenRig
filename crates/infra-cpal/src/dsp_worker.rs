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

/// Per-thread CPU time in nanoseconds — the time THIS thread actually spent
/// executing on a CPU, EXCLUDING any interval it was descheduled/preempted.
///
/// The worker's RT budget (#698) must be measured in COMPUTE time, not
/// wall-clock: a preemption stall (the kernel pulls the worker off-core
/// mid-DSP) inflates a wall-clock `Instant::elapsed` to multi-ms even though
/// the real DSP cost is microseconds. Feeding that inflated wall-clock to the
/// budget makes it re-declare the RT policy on every stall (a
/// `thread_policy_set` syscall that itself perturbs scheduling → more stalls).
/// `clock_gettime(CLOCK_THREAD_CPUTIME_ID)` advances ONLY while the thread is
/// running, so it is immune to preemption. `None` where unavailable (Windows);
/// callers fall back to wall-clock there.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn thread_cpu_time_ns() -> Option<u64> {
    #[repr(C)]
    struct Timespec {
        tv_sec: i64,
        tv_nsec: i64,
    }
    extern "C" {
        fn clock_gettime(clock_id: i32, ts: *mut Timespec) -> i32;
    }
    // CLOCK_THREAD_CPUTIME_ID: 16 on macOS (libSystem), 3 on Linux (glibc).
    #[cfg(target_os = "macos")]
    const CLOCK_THREAD_CPUTIME_ID: i32 = 16;
    #[cfg(target_os = "linux")]
    const CLOCK_THREAD_CPUTIME_ID: i32 = 3;
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { clock_gettime(CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    if rc != 0 {
        return None;
    }
    Some(ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn thread_cpu_time_ns() -> Option<u64> {
    None
}

/// Issue #698 — adaptive RT computation budget. Buffers measured per
/// window; at each window boundary the worker re-declares its
/// time-constraint computation from the measured worst case plus
/// headroom, so N concurrent chains together stay inside what the
/// kernel's admission will schedule. Plain data, worker-thread only.
pub(crate) struct BudgetTracker {
    /// Highest and second-highest measured cost in the current window. The
    /// budget is driven by the SECOND highest, so a single transient outlier
    /// (a preemption stall — the worker descheduled mid-DSP, which inflates
    /// the WALL-CLOCK measurement without being real compute cost) cannot move
    /// the budget. A genuine sustained cost increase shows up in the second
    /// highest too.
    window_max_ns: u64,
    window_2nd_ns: u64,
    window_count: u32,
    declared_ns: u64,
    /// Consecutive buffers measured over the declared budget. The fast
    /// up-correction fires only when this is SUSTAINED, never on a lone spike.
    consecutive_over: u32,
    /// #743: consecutive WINDOWS whose target sits below the declared budget,
    /// and the highest such target seen across that run. The budget shrinks only
    /// after a sustained run (never on a single low window), and settles to the
    /// run's high-water — so a steady but spiky load whose per-window cost
    /// alternates does not bounce the policy down and back up.
    low_run: u32,
    low_run_peak_ns: u64,
}

impl BudgetTracker {
    /// ≈3 s of buffers at 64 frames / 44.1 kHz between re-declarations.
    const WINDOW: u32 = 2048;
    /// A genuine cost increase persists; a preemption stall is one isolated
    /// buffer. Require this many CONSECUTIVE over-budget buffers before the
    /// fast up-correction re-declares — so a transient stall does not churn
    /// the RT policy (each re-declaration is a `thread_policy_set` syscall on
    /// the worker that itself perturbs its scheduling → more stalls).
    const SUSTAIN: u32 = 3;

    /// An idle/paused window measures (near) nothing — a drained chain's worker
    /// only copies the buffer and short-circuits, ~1-2 µs. Below 1% of the period
    /// the window carries no real cost signal, so the budget is left untouched;
    /// collapsing it to the floor only forces a fast-up re-declare the instant
    /// work resumes (#743). The threshold sits FAR under any measurable real
    /// workload (e.g. the #698 cheap-settle test's 50 µs ≈ 3.4% of the period),
    /// so genuine cheap chains still settle their budget down.
    const IDLE_NS_DIVISOR: u64 = 100;

    /// Windows of sustained below-budget cost required before the budget shrinks
    /// (≈11 s at 2048 buffers / 64 frames / 48 kHz). A shorter run is just
    /// normal window-to-window variance and must not re-declare (#743).
    const DOWN_SUSTAIN: u32 = 4;

    pub(crate) fn new(declared_ns: u64) -> Self {
        Self {
            window_max_ns: 0,
            window_2nd_ns: 0,
            window_count: 0,
            declared_ns,
            consecutive_over: 0,
            low_run: 0,
            low_run_peak_ns: 0,
        }
    }

    /// Record one processed buffer's cost; returns the new computation
    /// budget when the window closes AND it differs meaningfully from the
    /// declared one (hysteresis: 10% of the period).
    ///
    /// Fast up-correction: a SUSTAINED run of buffers over the declared budget
    /// means the chain genuinely got heavier (live rebuild, block added) and
    /// the kernel will demote an over-budget RT thread — re-declare the
    /// conservative 85% immediately instead of waiting for the window
    /// (measured: 128 underruns in the post-rebuild stretch of
    /// `rebuild_while_playing_keeps_the_cushion` without this). A SINGLE
    /// over-budget buffer is a preemption stall, not a cost change, and is
    /// ignored — otherwise the policy churns (the #698 single-chain crackle).
    pub(crate) fn observe(&mut self, elapsed_ns: u64, period_ns: u64) -> Option<u64> {
        if elapsed_ns > self.declared_ns {
            self.consecutive_over += 1;
        } else {
            self.consecutive_over = 0;
        }
        if self.consecutive_over >= Self::SUSTAIN && self.declared_ns < period_ns * 85 / 100 {
            self.consecutive_over = 0;
            return Some(self.reset(period_ns));
        }

        // Track the top two costs of the window.
        if elapsed_ns > self.window_max_ns {
            self.window_2nd_ns = self.window_max_ns;
            self.window_max_ns = elapsed_ns;
        } else if elapsed_ns > self.window_2nd_ns {
            self.window_2nd_ns = elapsed_ns;
        }
        self.window_count += 1;
        if self.window_count < Self::WINDOW {
            return None;
        }
        // Second-highest cost + 25% headroom, floored at 10% of the period (an
        // undersized budget also demotes — the reverted #670 attempt) and
        // capped at the validated 85%. Using the SECOND highest makes one
        // isolated preemption stall per window invisible to the budget.
        let robust = self.window_2nd_ns;
        self.window_max_ns = 0;
        self.window_2nd_ns = 0;
        self.window_count = 0;
        // #743: an idle/paused window (the worker measured ~nothing — a drained
        // chain) must NOT collapse the budget to the floor. Doing so only forces
        // the fast-up to re-declare the policy the instant work resumes, so every
        // pause/resume churns two `thread_policy_set` syscalls and the resulting
        // scheduling perturbation shows up as 4-6 ms late buffers. Keep the
        // standing budget across an idle window.
        if robust < period_ns / Self::IDLE_NS_DIVISOR {
            return None;
        }
        let target = (robust + robust / 4).clamp(period_ns / 10, period_ns * 85 / 100);
        let hyst = period_ns / 10;
        // Grow promptly: an under-budget RT thread gets demoted (#698 safety).
        if target > self.declared_ns + hyst {
            self.declared_ns = target;
            self.low_run = 0;
            self.low_run_peak_ns = 0;
            return Some(target);
        }
        // #743: the target sits meaningfully BELOW the standing budget. A single
        // low window is just variance — shrinking now only invites a re-grow next
        // window (the owner's 372 ↔ 592 µs steady-play churn). Shrink only after
        // a sustained run of low windows, and then to the run's HIGH-WATER so a
        // spiky-but-steady load settles to its peak instead of bouncing.
        if self.declared_ns > target + hyst {
            self.low_run += 1;
            self.low_run_peak_ns = self.low_run_peak_ns.max(target);
            if self.low_run >= Self::DOWN_SUSTAIN {
                let settled = self.low_run_peak_ns;
                self.declared_ns = settled;
                self.low_run = 0;
                self.low_run_peak_ns = 0;
                return Some(settled);
            }
            return None;
        }
        // Within hysteresis — the budget already fits the load.
        self.low_run = 0;
        self.low_run_peak_ns = 0;
        None
    }

    /// After a saturation spiral the chain's cost is unknown again —
    /// restart from the conservative cold-start budget.
    fn reset(&mut self, period_ns: u64) -> u64 {
        self.window_max_ns = 0;
        self.window_2nd_ns = 0;
        self.window_count = 0;
        self.consecutive_over = 0;
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
            crate::audio_workgroup::ensure_joined_input(device_uid.as_deref());
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

#[cfg(test)]
mod budget_tracker_tests {
    use super::BudgetTracker;

    const PERIOD_NS: u64 = 1_451_000; // 64 frames @ 44.1 kHz
    /// The real per-buffer DSP compute on an M4: microseconds. Far under any
    /// sane budget.
    const CHEAP_NS: u64 = 50_000;
    /// A transient PREEMPTION spike: the worker was descheduled mid-DSP, so the
    /// wall-clock `elapsed` reads multi-ms even though the COMPUTE was cheap.
    const PREEMPT_SPIKE_NS: u64 = 3_000_000;

    /// Count how many times `observe` re-declares the RT budget (returns Some)
    /// over a run of `n` buffers whose cost is `cost(i)`.
    fn count_redeclares(b: &mut BudgetTracker, n: usize, cost: impl Fn(usize) -> u64) -> usize {
        (0..n)
            .filter(|&i| b.observe(cost(i), PERIOD_NS).is_some())
            .count()
    }

    /// Issue (2026-06-17): the macOS RT dsp-worker stalled 2-3 ms intermittently
    /// on a SINGLE light chain on an idle M4 despite a successful RT promotion
    /// (rc=0), crackling under real load. Root cause A/B-confirmed on the real
    /// Scarlett (peak worker load ~9x → <1.6x with the re-budget disabled): the
    /// #698 adaptive BudgetTracker re-declares the RT time-constraint policy
    /// (`thread_policy_set`) on every buffer whose WALL-CLOCK cost spiked — but a
    /// wall-clock spike is PREEMPTION, not a real DSP cost increase. Each
    /// re-declaration is a syscall on the RT worker that perturbs its own
    /// scheduling → more stalls (a feedback loop).
    ///
    /// A steady, cheap workload with occasional transient preemption spikes must
    /// NOT churn the budget. (Deterministic, no hardware, no ear.)
    #[test]
    fn does_not_rebudget_on_transient_preemption_spikes() {
        let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);

        // Warm up to the real cheap cost. One legitimate down-declaration when
        // the first window closes is expected and fine.
        let _ = count_redeclares(&mut b, BudgetTracker::WINDOW as usize, |_| CHEAP_NS);

        // Steady cheap stream with a single transient preemption spike ONCE PER
        // window (one descheduled buffer every ~3 s of audio) — the realistic
        // shape: most buffers cheap, an occasional multi-ms preemption. The
        // workload did NOT get heavier; every spike is the worker being
        // descheduled, not real cost.
        let step = BudgetTracker::WINDOW as usize + 1;
        let redeclares = count_redeclares(&mut b, BudgetTracker::WINDOW as usize * 8, |i| {
            if i % step == 0 {
                PREEMPT_SPIKE_NS
            } else {
                CHEAP_NS
            }
        });

        assert!(
            redeclares <= 1,
            "BudgetTracker re-declared the RT budget {redeclares}x on a steady-cheap \
             workload with only transient preemption spikes. Each re-declaration is a \
             thread_policy_set on the RT worker that stalls it — the #698 single-chain \
             crackle. A transient wall-clock spike is preemption, not a real cost \
             increase, and must not re-budget."
        );
    }

    /// The other half of the contract: a GENUINE sustained cost increase (a
    /// block added, a real rebuild) MUST still re-declare promptly — the #698
    /// behaviour we keep. This guards against "fix the churn by never adapting".
    #[test]
    fn still_rebudgets_on_a_sustained_real_cost_increase() {
        let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);
        // Settle to the real cheap cost first. #743: the budget is sticky
        // downward, so it shrinks only after DOWN_SUSTAIN low windows — warm up
        // past that so `declared` actually reaches the cheap cost before the
        // increase (a 1-window warmup would leave it at the cold 85%).
        let warmup = BudgetTracker::WINDOW as usize * (BudgetTracker::DOWN_SUSTAIN as usize + 2);
        let _ = count_redeclares(&mut b, warmup, |_| CHEAP_NS);

        // Now the chain genuinely gets heavier and STAYS heavier (sustained,
        // not a one-off spike): ~60% of the period, every buffer.
        let heavy = PERIOD_NS * 60 / 100;
        let redeclares = count_redeclares(&mut b, BudgetTracker::WINDOW as usize, |_| heavy);
        assert!(
            redeclares >= 1,
            "a sustained real cost increase must re-declare the RT budget (the #698 \
             adaptation we keep), got {redeclares} re-declarations"
        );
    }

    /// A steady cheap workload re-declares the budget DOWN exactly once (to the
    /// real cost) and then SETTLES — the hysteresis must stop it re-declaring
    /// every window. (Re-declaration is a `thread_policy_set` syscall on the RT
    /// worker; needless ones perturb its scheduling.)
    #[test]
    fn steady_cheap_workload_settles_after_one_down_declaration() {
        let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);
        let redeclares = count_redeclares(&mut b, BudgetTracker::WINDOW as usize * 6, |_| CHEAP_NS);
        assert_eq!(
            redeclares, 1,
            "a steady cheap workload re-declares once (down to real cost) then settles"
        );
    }

    /// The RT budget must be measured in THREAD CPU TIME, not wall-clock, so a
    /// preemption stall (the worker descheduled mid-DSP) cannot be mistaken for
    /// DSP cost. This pins the property of `thread_cpu_time_ns`: a sleep (the
    /// thread NOT running — a stand-in for preemption) does NOT advance thread
    /// CPU time, while it fully advances wall-clock. Deterministic, no hardware.
    #[test]
    fn thread_cpu_time_excludes_preemption_sleep() {
        let Some(c0) = super::thread_cpu_time_ns() else {
            return; // platform without per-thread CPU clock — fallback path
        };
        let wall0 = std::time::Instant::now();

        // Stand-in for preemption: the thread is descheduled (sleeping) — not
        // computing — for 50 ms.
        std::thread::sleep(std::time::Duration::from_millis(50));
        // A little real compute so thread CPU time advances measurably.
        let mut acc = 0u64;
        for i in 0..2_000_000u64 {
            acc = acc.wrapping_add(i.rotate_left(7));
        }
        std::hint::black_box(acc);

        let compute_ns = super::thread_cpu_time_ns().unwrap() - c0;
        let wall_ns = wall0.elapsed().as_nanos() as u64;

        assert!(
            wall_ns >= 50_000_000,
            "wall-clock {wall_ns}ns must include the 50ms sleep"
        );
        assert!(
            compute_ns < 40_000_000,
            "thread CPU time {compute_ns}ns must EXCLUDE the 50ms preemption sleep — \
             this is why the RT budget is measured in compute time: wall-clock would \
             attribute a preemption stall as DSP cost and churn the budget."
        );
    }
}

#[cfg(test)]
mod saturation_recovery_tests {
    use super::SaturationRecovery;

    /// Recovery (re-promote + drop backlog, #670 death-spiral break) must fire
    /// ONLY after `threshold` CONSECUTIVE saturated drains, then reset — never
    /// on a transient saturation that the ring recovers from on its own.
    #[test]
    fn fires_only_after_threshold_consecutive_saturations() {
        let mut r = SaturationRecovery::new(3);
        assert!(!r.observe(true), "1st saturation: not yet");
        assert!(!r.observe(true), "2nd saturation: not yet");
        assert!(r.observe(true), "3rd consecutive saturation: recover NOW");
        // After firing it restarts the run.
        assert!(!r.observe(true), "run restarted after firing");
        assert!(!r.observe(true));
        assert!(r.observe(true), "next 3-run fires again");
    }

    #[test]
    fn a_single_healthy_drain_resets_the_run() {
        let mut r = SaturationRecovery::new(3);
        assert!(!r.observe(true));
        assert!(!r.observe(true));
        // A healthy (non-saturated) drain breaks the streak — the spiral
        // resolved on its own, so recovery must NOT fire.
        assert!(!r.observe(false), "healthy drain resets, no recovery");
        assert!(!r.observe(true), "streak restarts from zero");
        assert!(!r.observe(true));
        assert!(r.observe(true), "needs a fresh full streak to fire");
    }

    #[test]
    fn threshold_one_fires_on_every_saturation() {
        let mut r = SaturationRecovery::new(1);
        assert!(r.observe(true));
        assert!(r.observe(true));
        assert!(!r.observe(false));
    }
}
