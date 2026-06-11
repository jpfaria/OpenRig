//! Issue #698 — sampling the owner's live GUI process during his crackles
//! showed `malloc`/`free` INSIDE `nam::Conv1D::Process` / Eigen GEMM stacks
//! on the DSP worker (Eigen allocates its level-3 blocking workspace per
//! product). Invariant #8 says the audio path allocates NOTHING — because
//! macOS malloc zones take a process-wide lock, and in the real app 25+
//! GUI/tokio/MIDI threads hit the allocator continuously. The RT worker
//! then blocks mid-buffer for milliseconds (the owner's 2-11 ms stalls,
//! 1451 us budget).
//!
//! This test is the owner's symptom in miniature, no audio device needed:
//! process 64-frame buffers at the NAM block level while background
//! threads hammer the allocator the way the GUI process does. If NAM's
//! inference touches malloc, the worker inherits the contention and the
//! per-buffer tail blows past the real-time budget — RED. Once the
//! inference is allocation-free, the storm is irrelevant — GREEN.
#![cfg(not(debug_assertions))]

use block_core::MonoProcessor;
use nam::processor::{NamProcessor, DEFAULT_PLUGIN_PARAMS};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// 64 frames @ 44.1 kHz — the owner's configured budget.
const PERIOD_NS: u128 = 64 * 1_000_000_000 / 44_100;
const STORM_THREADS: usize = 6;
const MEASURE_BUFFERS: usize = 5_000;

fn od808() -> String {
    format!(
        "{}/../engine/tests/fixtures/plugins/nam/maxon_od808_a2/captures/od808_2pm_2pm_plus6_a2.nam",
        env!("CARGO_MANIFEST_DIR")
    )
}

/// Promote the measuring thread to the macOS realtime (time-constraint)
/// class, exactly like the production dsp-worker. Without this the test
/// thread is preempted by the storm threads (and anything else on the
/// machine) and the measurement captures SCHEDULING noise; with it, only
/// what the production worker also suffers — a lock held by a lower
/// priority thread (the malloc zone lock) — can stall the loop. That
/// inheritance is precisely the defect under test.
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
        eprintln!("[#698 MALLOC-STORM] measuring-thread realtime promotion rc={rc}");
    }
}

#[test]
fn nam_block_holds_the_64_frame_budget_under_allocator_contention() {
    let mut params = DEFAULT_PLUGIN_PARAMS;
    params.noise_gate_enabled = false;
    let mut proc =
        NamProcessor::new(&od808(), None, params, 44_100.0).expect("od808 A2 must load");
    let mut buf: Vec<f32> = (0..64)
        .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 44_100.0).sin())
        .collect();
    // Warm-up: cold caches, lazy first-call growth.
    for _ in 0..512 {
        proc.process_block(&mut buf);
    }
    promote_to_audio_rt(PERIOD_NS as u64);

    // The GUI process in miniature: threads that allocate and free
    // continuously (Slint props, tokio tasks, meter strings, log lines).
    let stop = Arc::new(AtomicBool::new(false));
    let storm: Vec<_> = (0..STORM_THREADS)
        .map(|t| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut keep: Vec<Vec<u8>> = Vec::with_capacity(64);
                let mut i = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    // Vary the size class so the zone's small/tiny locks all
                    // get exercised, like a real mixed workload.
                    let size = 32 + ((i * 37 + t * 101) % 4096);
                    keep.push(vec![0u8; size]);
                    if keep.len() == 64 {
                        keep.clear();
                    }
                    i = i.wrapping_add(1);
                    std::hint::black_box(&keep);
                }
            })
        })
        .collect();

    let mut samples = Vec::with_capacity(MEASURE_BUFFERS);
    for _ in 0..MEASURE_BUFFERS {
        let t0 = Instant::now();
        proc.process_block(&mut buf);
        samples.push(t0.elapsed().as_nanos());
    }

    stop.store(true, Ordering::Relaxed);
    for s in storm {
        let _ = s.join();
    }

    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p99 = samples[samples.len() * 99 / 100];
    let max = *samples.last().unwrap();
    let over_budget = samples.iter().filter(|&&s| s > PERIOD_NS).count();
    eprintln!(
        "[#698 MALLOC-STORM] od808 A2, 64 frames @ 44.1 kHz under {STORM_THREADS}-thread \
         allocator storm: median={}us p99={}us max={}us over-budget={}/{}",
        median / 1000,
        p99 / 1000,
        max / 1000,
        over_budget,
        MEASURE_BUFFERS,
    );

    assert!(
        p99 < PERIOD_NS && over_budget < MEASURE_BUFFERS / 100,
        "BUG #698: NAM inference stalls under allocator contention — p99 {}us, \
         max {}us against the {}us budget; {over_budget}/{MEASURE_BUFFERS} buffers \
         over budget. The sampled stacks show Eigen GEMM malloc/free inside \
         nam::Conv1D::Process: the audio worker takes the process-wide malloc \
         lock and inherits every other thread's allocator traffic (the owner's \
         crackles in the GUI process, absent headless).",
        p99 / 1000,
        max / 1000,
        PERIOD_NS / 1000,
    );
}
