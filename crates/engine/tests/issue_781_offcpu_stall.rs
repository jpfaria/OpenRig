//! Issue #781 — pin the RESIDUAL underrun's mechanism as OFF-CPU vs ON-CPU,
//! headless, with the user's REAL rig:input-1 chain (gate + 2 NAM + eq8).
//!
//! After the #743 stream-isolation fix the user still hears 128-192 sporadic
//! underruns "em qualquer situação" — even ONE chain, one worker, on an idle
//! M4, with the VST3 DEACTIVATED. The live worker trace of a late buffer reads
//! `~350us thread-cpu / ~1590us wall`: the DSP call spent ~1240us OFF the CPU
//! (thread descheduled / blocked / faulting) DURING an alloc-free process (the
//! #670/#781 alloc battery proves zero heap allocs through this exact chain).
//!
//! This test measures, per buffer, BOTH the wall-clock AND the thread CPU time
//! (mach `thread_info`) around the real `process_input_f32` + `process_output_f32`
//! — exactly what the worker measures — and also the process's `pageins`
//! (mach `task_info`). It runs SINGLE-THREADED with NO contention (the user's
//! "single chain, idle machine" case).
//!
//!   - If the DSP is ~fully ON-CPU (wall ≈ cpu, pageins flat): the stall is NOT
//!     intrinsic to our DSP path — it needs the real worker / device / other
//!     threads (points at scheduling or memory pressure from the GUI).
//!   - If OFF-CPU stalls appear here with zero contention: the mechanism is
//!     intrinsic — a per-buffer page-fault (NAM weights) or a hidden syscall in
//!     the process path. `pageins` rising during the steady window names it.
//!
//! GATING: `--release` (debug NAM is far slower than realtime). macOS-only for
//! the thread/task CPU probes. Run:
//!   cargo test -p engine --release --test issue_781_offcpu_stall -- --nocapture
#![cfg(all(not(debug_assertions), target_os = "macos"))]

use std::path::PathBuf;
use std::sync::{Arc, Once};
use std::time::Instant;

use block_core::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};
use domain::value_objects::ParameterValue;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::runtime_graph::build_chain_runtime_state;
use engine::runtime_state::ChainRuntimeState;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, NamBlock};
use project::chain::Chain;

const SR: f32 = 48_000.0;
const BUFFER: usize = 64;
const PERIOD_US: u128 = (BUFFER as u128) * 1_000_000 / 48_000; // 1333us
const ITERS: usize = 8_000;
const WARMUP: usize = 512;

// ── mach probes: thread CPU time + process pageins ───────────────────────────

#[repr(C)]
struct TimeValue {
    seconds: i32,
    microseconds: i32,
}
#[repr(C)]
struct ThreadBasicInfo {
    user_time: TimeValue,
    system_time: TimeValue,
    cpu_usage: i32,
    policy: i32,
    run_state: i32,
    flags: i32,
    suspend_count: i32,
    sleep_time: i32,
}
#[repr(C)]
struct TaskEventsInfo {
    faults: i32,
    pageins: i32,
    cow_faults: i32,
    messages_sent: i32,
    messages_received: i32,
    syscalls_mach: i32,
    syscalls_unix: i32,
    csw: i32,
}

extern "C" {
    fn mach_thread_self() -> u32;
    fn mach_task_self() -> u32;
    fn thread_info(thread: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    fn task_info(task: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
}

/// Current thread's consumed CPU time (user+system) in microseconds. Excludes
/// any time the thread was descheduled — so `wall - cpu` is the OFF-CPU time.
fn thread_cpu_us() -> u128 {
    const THREAD_BASIC_INFO: i32 = 3;
    const COUNT: u32 = (std::mem::size_of::<ThreadBasicInfo>() / std::mem::size_of::<i32>()) as u32;
    let mut info = ThreadBasicInfo {
        user_time: TimeValue { seconds: 0, microseconds: 0 },
        system_time: TimeValue { seconds: 0, microseconds: 0 },
        cpu_usage: 0,
        policy: 0,
        run_state: 0,
        flags: 0,
        suspend_count: 0,
        sleep_time: 0,
    };
    let mut count = COUNT;
    unsafe {
        thread_info(
            mach_thread_self(),
            THREAD_BASIC_INFO,
            &mut info as *mut _ as *mut i32,
            &mut count,
        );
    }
    let u = info.user_time.seconds as u128 * 1_000_000 + info.user_time.microseconds as u128;
    let s = info.system_time.seconds as u128 * 1_000_000 + info.system_time.microseconds as u128;
    u + s
}

/// Process-wide (pageins, csw) counters — pageins rising in the steady window
/// means the audio path is faulting memory back in (the #715 weights, off-CPU).
fn task_pageins_csw() -> (i64, i64) {
    const TASK_EVENTS_INFO: i32 = 2;
    const COUNT: u32 = (std::mem::size_of::<TaskEventsInfo>() / std::mem::size_of::<i32>()) as u32;
    let mut info = TaskEventsInfo {
        faults: 0,
        pageins: 0,
        cow_faults: 0,
        messages_sent: 0,
        messages_received: 0,
        syscalls_mach: 0,
        syscalls_unix: 0,
        csw: 0,
    };
    let mut count = COUNT;
    unsafe {
        task_info(
            mach_task_self(),
            TASK_EVENTS_INFO,
            &mut info as *mut _ as *mut i32,
            &mut count,
        );
    }
    (info.pageins as i64, info.csw as i64)
}

// ── the user's real rig:input-1 chain ────────────────────────────────────────

fn plugins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins")
}

fn init_registry() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        nam::register_builder();
        ir::register_builder();
        block_gain::register_natives();
        block_amp::register_natives();
        block_dyn::register_natives();
        block_filter::register_natives();
        plugin_loader::registry::init(&plugins_root());
    });
}

fn registry() -> Vec<domain::io_binding::IoBinding> {
    vec![domain::io_binding::IoBinding {
        id: "io".into(),
        name: "IO".into(),
        inputs: vec![domain::io_binding::IoEndpoint {
            name: "in0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![domain::io_binding::IoEndpoint {
            name: "out0".into(),
            device_id: DeviceId("dev".into()),
            mode: domain::io_binding::ChannelMode::Stereo,
            channels: vec![0, 1],
        }],
    }]
}

fn floats(pairs: &[(&str, f32)]) -> ParameterSet {
    let mut p = ParameterSet::default();
    for (k, v) in pairs {
        p.insert(*k, ParameterValue::Float(*v));
    }
    p
}

fn core(id: &str, et: &str, model: &str, p: ParameterSet) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: et.into(),
            model: model.into(),
            params: p,
        }),
    }
}

fn nam(id: &str, model: &str, preset: &str) -> AudioBlock {
    let mut p = ParameterSet::default();
    p.insert("preset", ParameterValue::String(preset.into()));
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Nam(NamBlock { model: model.into(), params: p }),
    }
}

fn eq8() -> AudioBlock {
    let mut p = ParameterSet::default();
    for b in 1..=8 {
        p.insert(format!("band{b}_enabled"), ParameterValue::Bool(true));
        p.insert(format!("band{b}_freq"), ParameterValue::Float(1000.0));
        p.insert(format!("band{b}_gain"), ParameterValue::Float(0.0));
        p.insert(format!("band{b}_q"), ParameterValue::Float(1.0));
        p.insert(format!("band{b}_type"), ParameterValue::String("peak".into()));
    }
    p.insert("output_db", ParameterValue::Float(0.0));
    core("eq8", "filter", "eq_eight_band_parametric", p)
}

fn gate() -> AudioBlock {
    core(
        "gate",
        "dynamics",
        "gate_basic",
        floats(&[
            ("threshold", -60.0),
            ("attack_ms", 1.0),
            ("release_ms", 100.0),
            ("hold_ms", 150.0),
            ("hysteresis_db", 6.0),
        ]),
    )
}

fn build() -> Arc<ChainRuntimeState> {
    let chain = Chain {
        id: ChainId("issue-781-rig-input1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![
            gate(),
            nam("amp1", "nam_marshall_plexi", "angus"),
            nam("amp2", "nam_marshall_plexi", "angus"),
            eq8(),
        ],
        di_output: None,
        loopers: vec![],
    };
    Arc::new(build_chain_runtime_state(&chain, SR, &[BUFFER], &registry()).expect("build rig chain"))
}

#[test]
fn residual_underrun_is_offcpu_or_oncpu_on_the_real_rig() {
    if std::env::var_os("OPENRIG_HW_TESTS").is_none() {
        eprintln!("[#781 offcpu] SKIPPED — set OPENRIG_HW_TESTS=1 (timing probe, idle machine).");
        return;
    }
    init_registry();
    let rt = build();

    let input = vec![0.1_f32; BUFFER];
    let mut out = vec![0.0_f32; BUFFER * 2];
    for _ in 0..WARMUP {
        process_input_f32(&rt, 0, &input, 1);
        process_output_f32(&rt, 0, &mut out, 2);
    }

    let p = |v: &[u128], q: usize| v[(v.len() * q / 100).min(v.len() - 1)];

    // ── Pass A: TIGHT loop (back-to-back). The NAM working set stays HOT in
    // cache the whole time — the DSP's floor cost, nothing cools.
    let mut hot: Vec<u128> = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let c0 = thread_cpu_us();
        process_input_f32(&rt, 0, &input, 1);
        process_output_f32(&rt, 0, &mut out, 2);
        hot.push(thread_cpu_us().saturating_sub(c0));
    }
    hot.sort_unstable();

    // ── Pass B: REAL CADENCE. Process ONCE per period, then sleep the rest of
    // the period — exactly the worker's duty cycle. Between buffers the NAM
    // weights cool out of cache; the next inference pays the cold-cache tail
    // (#715/#670). This is what the worker's `351us cpu / 1587us wall` trace is,
    // and what the tight loop above CANNOT show. Measure compute AND off-cpu.
    let period = std::time::Duration::from_micros(PERIOD_US as u64);
    let (pageins0, csw0) = task_pageins_csw();
    let mut cold_cpu: Vec<u128> = Vec::with_capacity(ITERS);
    let mut cold_wall: Vec<u128> = Vec::with_capacity(ITERS);
    let mut cold_off: Vec<u128> = Vec::with_capacity(ITERS);
    let mut next = Instant::now();
    for _ in 0..ITERS {
        let c0 = thread_cpu_us();
        let w0 = Instant::now();
        process_input_f32(&rt, 0, &input, 1);
        process_output_f32(&rt, 0, &mut out, 2);
        let wall = w0.elapsed().as_micros();
        let cpu = thread_cpu_us().saturating_sub(c0);
        cold_cpu.push(cpu);
        cold_wall.push(wall);
        cold_off.push(wall.saturating_sub(cpu));
        next += period;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }
    }
    let (pageins1, csw1) = task_pageins_csw();
    cold_cpu.sort_unstable();
    cold_wall.sort_unstable();
    cold_off.sort_unstable();

    let cold_late = cold_wall.iter().filter(|&&w| w > PERIOD_US).count();

    eprintln!("[#781 offcpu] period={PERIOD_US}us  iters={ITERS}  (single-thread, NO contention)");
    eprintln!(
        "[#781 offcpu] A) HOT tight loop     compute: median={}us  p99={}us  p999={}us",
        p(&hot, 50), p(&hot, 99), p(&hot, 999 / 10)
    );
    eprintln!(
        "[#781 offcpu] B) REAL cadence       compute: median={}us  p99={}us  p999={}us  MAX={}us",
        p(&cold_cpu, 50), p(&cold_cpu, 99), p(&cold_cpu, 999 / 10), cold_cpu[cold_cpu.len() - 1]
    );
    eprintln!(
        "[#781 offcpu] B) REAL cadence       wall   : median={}us  p99={}us  MAX={}us  late(>period)={cold_late}",
        p(&cold_wall, 50), p(&cold_wall, 99), cold_wall[cold_wall.len() - 1]
    );
    eprintln!(
        "[#781 offcpu] B) REAL cadence      off-cpu : median={}us  p99={}us  MAX={}us",
        p(&cold_off, 50), p(&cold_off, 99), cold_off[cold_off.len() - 1]
    );
    eprintln!(
        "[#781 offcpu] cold/hot compute p99 ratio = {:.1}x   pageins Δ={}  csw Δ={}",
        p(&cold_cpu, 99) as f64 / p(&hot, 99).max(1) as f64,
        pageins1 - pageins0,
        csw1 - csw0
    );

    // The reproduction: at the REAL duty cycle the periodic cadence lets the NAM
    // working set cool, and the cold-cache inference tail inflates the compute
    // well past the hot floor — the residual underrun's mechanism, headless, no
    // device, no contention. Direction only (absolute timing is machine-dep).
    assert!(
        p(&cold_cpu, 99) > p(&hot, 99),
        "expected the real periodic cadence to inflate NAM compute vs the hot \
         tight loop (cold-cache tail) — got cold p99 {}us <= hot p99 {}us; the \
         working set is not cooling on this machine.",
        p(&cold_cpu, 99),
        p(&hot, 99)
    );
}
