//! Headless reproduction of the worker "late buffer" the user sees in the
//! live app (issue #715). NO audio device, NO GUI, NO ear: a thread that does
//! a fixed ~50µs of "DSP" once per 64-frame/44.1kHz period (1451µs), under CPU
//! contention that stands in for the app's UI/render threads competing for the
//! cores. We COUNT how often a "buffer" takes longer than its period — exactly
//! the `[#670 worker] late buffer` log line.
//!
//! The point the user made: the input is a DI loop (deterministic), so this IS
//! simulable — what varies between my idle test box and their machine is the
//! CONTENTION, which we inject here. The variants show the mechanism:
//!   - a NORMAL (non-RT) thread under contention → many late buffers
//!   - an RT-promoted (Mach time-constraint) thread → fewer
//!
//! The remaining gap (RT thread STILL late under heavy contention) is exactly
//! why the worker needs the CoreAudio workgroup (P-core coscheduling) — which
//! requires a real device, so it is validated in the HW battery, not here.
//!
//! Run: `cargo test -p infra-cpal --release --test worker_preemption_repro -- --nocapture`
#![cfg(target_os = "macos")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const PERIOD_US: u64 = 1451; // 64 frames @ 44.1 kHz
const COMPUTE_US: u64 = 50; // the real per-buffer DSP cost on an M4 (microseconds)
const SECONDS: u64 = 8;

/// Mach time-constraint promotion — the same policy the dsp-worker uses.
fn promote_to_audio_rt(period_ns: u64, computation_ns: u64, preemptible: u32) {
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
            computation: to_mach(computation_ns),
            constraint: to_mach(period_ns),
            preemptible,
        };
        thread_policy_set(
            mach_thread_self(),
            THREAD_TIME_CONSTRAINT_POLICY,
            &policy as *const _ as *const u32,
            4,
        );
    }
}

/// Burn `us` microseconds of REAL CPU (busy loop — not a sleep, so preemption
/// inflates the wall-clock around it just like real DSP).
#[inline(never)]
fn burn(us: u64) {
    let until = Instant::now() + Duration::from_micros(us);
    let mut x = 0.0001f64;
    while Instant::now() < until {
        for _ in 0..64 {
            x = (x.sin().cos() + 1.0001).sqrt();
        }
        std::hint::black_box(x);
    }
}

/// Run one "worker": once per period, do `COMPUTE_US` of work and count a late
/// buffer when the work's wall-clock exceeded the period (the thread was
/// descheduled mid-compute). Returns (late_count, max_late_us).
fn run_worker(rt: Option<u32>, contenders: usize) -> (u64, u64) {
    let period = Duration::from_micros(PERIOD_US);
    let period_ns = PERIOD_US * 1000;

    let stop = Arc::new(AtomicBool::new(false));
    let loaders: Vec<_> = (0..contenders)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut x = 0.001f64;
                while !stop.load(Ordering::Relaxed) {
                    x = (x.sin().cos() + 1.0001).sqrt();
                    std::hint::black_box(x);
                }
            })
        })
        .collect();

    let worker = std::thread::spawn(move || {
        if let Some(preemptible) = rt {
            promote_to_audio_rt(period_ns, COMPUTE_US * 1000 * 85 / 100, preemptible);
        }
        let mut late = 0u64;
        let mut max_late = 0u64;
        let end = Instant::now() + Duration::from_secs(SECONDS);
        let mut next = Instant::now();
        while Instant::now() < end {
            let start = Instant::now();
            burn(COMPUTE_US);
            let elapsed = start.elapsed();
            if elapsed > period {
                late += 1;
                max_late = max_late.max(elapsed.as_micros() as u64);
            }
            next += period;
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else {
                next = now; // fell behind; resync
            }
        }
        (late, max_late)
    });

    let result = worker.join().unwrap();
    stop.store(true, Ordering::Relaxed);
    for l in loaders {
        let _ = l.join();
    }
    result
}

#[test]
fn late_buffers_reproduce_under_contention() {
    if std::env::var_os("OPENRIG_HW_TESTS").is_none() {
        eprintln!(
            "[preempt-repro] SKIPPED — set OPENRIG_HW_TESTS=1 (CPU-contention timing, idle machine)."
        );
        return;
    }
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    let contenders = cores * 2; // oversubscribe, like the app + browser + OS

    let total_buffers = SECONDS * 1_000_000 / PERIOD_US;
    eprintln!(
        "[preempt-repro] {SECONDS}s, period {PERIOD_US}us, compute {COMPUTE_US}us, {contenders} contenders (~{total_buffers} buffers/run)"
    );

    let (late_normal, max_normal) = run_worker(None, contenders);
    eprintln!("[preempt-repro] NORMAL thread     : late={late_normal} maxLate={max_normal}us");

    let (late_rt, max_rt) = run_worker(Some(1), contenders);
    eprintln!("[preempt-repro] RT preemptible=1   : late={late_rt} maxLate={max_rt}us");

    let (late_rt0, max_rt0) = run_worker(Some(0), contenders);
    eprintln!("[preempt-repro] RT preemptible=0   : late={late_rt0} maxLate={max_rt0}us");

    // The reproduction: a contended thread DOES produce late buffers (the
    // user's log). This is the headless simulation the user asked for.
    assert!(
        late_normal > 0,
        "expected to REPRODUCE late buffers on a contended normal thread — got 0; \
         the contention model is not loading the cores"
    );
    // Document (not assert — scheduling is machine-dependent) whether RT and
    // non-preemptible reduce it; the numbers are printed for the A/B.
    let _ = (late_rt, max_rt, late_rt0, max_rt0);
}
