//! Responsibility: promotes the worker thread to the realtime class.

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
pub(crate) fn promote_to_audio_rt(period_ns: u64, computation_ns: u64) {
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
        log::info!("dsp-worker realtime promotion rc={rc}");
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn promote_to_audio_rt(_period_ns: u64, _computation_ns: u64) {}

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
pub(crate) fn thread_cpu_time_ns() -> Option<u64> {
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
pub(crate) fn thread_cpu_time_ns() -> Option<u64> {
    None
}
