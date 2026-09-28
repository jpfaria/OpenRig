//! Test-only probe (#980): watches this process's `dsp-worker:*` threads from
//! the outside through the Mach API — current and base priority, scheduling
//! policy, run state and the declared time-constraint policy — and logs every
//! change with a millisecond timestamp on the `probe980` log target, so it
//! interleaves with the dsp-worker's own late-buffer trace.
//!
//! Nothing here touches the audio path: it runs on its own ordinary thread and
//! only reads kernel state. macOS only.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

#[repr(C)]
struct Timebase {
    numer: u32,
    denom: u32,
}

/// `struct thread_extended_info` (mach/thread_info.h).
#[repr(C)]
struct ThreadExtendedInfo {
    pth_user_time: u64,
    pth_system_time: u64,
    pth_cpu_usage: i32,
    pth_policy: i32,
    pth_run_state: i32,
    pth_flags: i32,
    pth_sleep_time: i32,
    pth_curpri: i32,
    pth_priority: i32,
    pth_maxpriority: i32,
    pth_name: [u8; 64],
}

const THREAD_EXTENDED_INFO: i32 = 5;
const THREAD_EXTENDED_INFO_COUNT: u32 = (std::mem::size_of::<ThreadExtendedInfo>() / 4) as u32;
const THREAD_TIME_CONSTRAINT_POLICY: i32 = 2;

extern "C" {
    static mach_task_self_: u32;
    fn task_threads(task: u32, list: *mut *mut u32, count: *mut u32) -> i32;
    fn thread_info(thread: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    fn thread_policy_get(
        thread: u32,
        flavor: i32,
        info: *mut u32,
        count: *mut u32,
        get_default: *mut i32,
    ) -> i32;
    fn mach_port_deallocate(task: u32, name: u32) -> i32;
    fn vm_deallocate(task: u32, address: usize, size: usize) -> i32;
    fn mach_timebase_info(info: *mut Timebase) -> i32;
}

/// One worker's observable scheduling state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct State {
    curpri: i32,
    priority: i32,
    policy: i32,
    computation_us: u64,
    period_us: u64,
}

pub struct Watch {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<HashMap<String, HashMap<i32, u64>>>>,
}

impl Watch {
    /// Stops the watcher and returns, per worker, how many samples saw each
    /// current priority.
    pub fn finish(mut self) -> HashMap<String, HashMap<i32, u64>> {
        self.stop.store(true, Ordering::Relaxed);
        self.handle
            .take()
            .map(|h| h.join().unwrap_or_default())
            .unwrap_or_default()
    }
}

/// Starts sampling every `period`.
pub fn start(period: Duration) -> Watch {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let handle = std::thread::Builder::new()
        .name("probe980-watch".into())
        .spawn(move || run(flag, period))
        .expect("spawn watcher");
    Watch {
        stop,
        handle: Some(handle),
    }
}

extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut u64) -> i32;
    fn getpid() -> i32;
}
const RUSAGE_INFO_V6: i32 = 6;
// u64 word offsets in `struct rusage_info_v6` (the 16-byte uuid is words 0-1).
const RI_USER_TIME: usize = 2;
const RI_SYSTEM_TIME: usize = 3;
const RI_INSTRUCTIONS: usize = 31;
const RI_CYCLES: usize = 32;
const RI_USER_PTIME: usize = 38;
const RI_SYSTEM_PTIME: usize = 39;
const RI_PCYCLES: usize = 41;

/// Whole-process CPU split between performance and efficiency cores.
#[derive(Clone, Copy)]
struct Usage {
    cpu_abs: u64,
    p_cpu_abs: u64,
    cycles: u64,
    p_cycles: u64,
    instructions: u64,
}

fn usage() -> Option<Usage> {
    let mut buf = [0u64; 96];
    let rc = unsafe { proc_pid_rusage(getpid(), RUSAGE_INFO_V6, buf.as_mut_ptr()) };
    (rc == 0).then(|| Usage {
        cpu_abs: buf[RI_USER_TIME] + buf[RI_SYSTEM_TIME],
        p_cpu_abs: buf[RI_USER_PTIME] + buf[RI_SYSTEM_PTIME],
        cycles: buf[RI_CYCLES],
        p_cycles: buf[RI_PCYCLES],
        instructions: buf[RI_INSTRUCTIONS],
    })
}

fn run(stop: Arc<AtomicBool>, period: Duration) -> HashMap<String, HashMap<i32, u64>> {
    let mut tb = Timebase { numer: 0, denom: 0 };
    unsafe { mach_timebase_info(&mut tb) };
    let to_us = |abs: u32| (abs as u64 * tb.numer as u64 / tb.denom.max(1) as u64) / 1_000;
    let abs_to_ns = |abs: u64| abs as u128 * tb.numer as u128 / tb.denom.max(1) as u128;
    let mut last: HashMap<String, State> = HashMap::new();
    let mut histogram: HashMap<String, HashMap<i32, u64>> = HashMap::new();
    let mut last_usage = usage();
    let mut ticks = 0u64;
    while !stop.load(Ordering::Relaxed) {
        ticks += 1;
        // Every ~100 ms: how much of the process's CPU ran on performance
        // cores, and at what effective clock on each core type.
        if ticks % 100 == 0 {
            if let (Some(prev), Some(now)) = (last_usage, usage()) {
                let cpu_ns = abs_to_ns(now.cpu_abs - prev.cpu_abs);
                let p_ns = abs_to_ns(now.p_cpu_abs - prev.p_cpu_abs);
                let e_ns = cpu_ns.saturating_sub(p_ns);
                let cyc = (now.cycles - prev.cycles) as u128;
                let p_cyc = (now.p_cycles - prev.p_cycles) as u128;
                let e_cyc = cyc.saturating_sub(p_cyc);
                let ghz = |c: u128, ns: u128| if ns == 0 { 0.0 } else { c as f64 / ns as f64 };
                log::info!(
                    target: "probe980",
                    "rusage 100ms: cpu {:.1} ms (P {:.0}%) | P {:.2} GHz, E {:.2} GHz | IPC {:.2}",
                    cpu_ns as f64 / 1e6,
                    if cpu_ns == 0 { 0.0 } else { p_ns as f64 * 100.0 / cpu_ns as f64 },
                    ghz(p_cyc, p_ns),
                    ghz(e_cyc, e_ns),
                    if cyc == 0 { 0.0 } else { (now.instructions - prev.instructions) as f64 / cyc as f64 }
                );
                last_usage = Some(now);
            }
        }
        for (name, state) in sample_workers(&to_us) {
            *histogram
                .entry(name.clone())
                .or_default()
                .entry(state.curpri)
                .or_default() += 1;
            if last.get(&name) != Some(&state) {
                log::info!(
                    target: "probe980",
                    "{name}: curpri {} base {} policy {} declared computation {}us / period {}us",
                    state.curpri,
                    state.priority,
                    state.policy,
                    state.computation_us,
                    state.period_us
                );
                last.insert(name, state);
            }
        }
        std::thread::sleep(period);
    }
    histogram
}

fn sample_workers(to_us: &dyn Fn(u32) -> u64) -> Vec<(String, State)> {
    let mut out = Vec::new();
    unsafe {
        let task = mach_task_self_;
        let mut list: *mut u32 = std::ptr::null_mut();
        let mut count: u32 = 0;
        if task_threads(task, &mut list, &mut count) != 0 || list.is_null() {
            return out;
        }
        let threads = std::slice::from_raw_parts(list, count as usize);
        for &thread in threads {
            let mut info: ThreadExtendedInfo = std::mem::zeroed();
            let mut info_count = THREAD_EXTENDED_INFO_COUNT;
            if thread_info(
                thread,
                THREAD_EXTENDED_INFO,
                &mut info as *mut _ as *mut i32,
                &mut info_count,
            ) == 0
            {
                let len = info.pth_name.iter().position(|&b| b == 0).unwrap_or(64);
                let name = String::from_utf8_lossy(&info.pth_name[..len]).to_string();
                if name.starts_with("dsp-worker") {
                    let mut policy = [0u32; 4];
                    let mut policy_count = 4u32;
                    let mut get_default = 0i32;
                    let _ = thread_policy_get(
                        thread,
                        THREAD_TIME_CONSTRAINT_POLICY,
                        policy.as_mut_ptr(),
                        &mut policy_count,
                        &mut get_default,
                    );
                    out.push((
                        format!("{name}#{thread}"),
                        State {
                            curpri: info.pth_curpri,
                            priority: info.pth_priority,
                            policy: info.pth_policy,
                            computation_us: if get_default != 0 {
                                0
                            } else {
                                to_us(policy[1])
                            },
                            period_us: if get_default != 0 {
                                0
                            } else {
                                to_us(policy[0])
                            },
                        },
                    ));
                }
            }
            mach_port_deallocate(task, thread);
        }
        vm_deallocate(
            task,
            list as usize,
            count as usize * std::mem::size_of::<u32>(),
        );
    }
    out
}
