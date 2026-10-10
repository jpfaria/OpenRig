//! Responsibility: runs every path of a split but the first on a thread of its own.
//!
//! One worker carrying every path of a split serially saturates one core
//! (two NAM amps in a Split → Mix measured one realtime thread at 95%, heard
//! as crackle). Each path but the first gets a lane: a thread spawned at
//! build, parked between callbacks, that runs its path while the split's own
//! worker runs the first path. The worker waits for every lane before it
//! mixes, so a lane only ever touches its path while the worker is inside
//! `run_paths`. The handshake is atomic, with no lock and no allocation; an
//! idle lane parks and is woken with `Thread::unpark`, the one syscall the
//! audio path allows, so it uses no CPU between callbacks.
//!
//! The worker spins while it waits for its lanes, on purpose: it declares
//! its realtime budget from the CPU it measures on itself, and its lanes
//! take that budget. A worker that parked would measure only its own path,
//! and a lane running a costlier one would overrun the budget it inherits;
//! the kernel demotes a thread that does, heard as underruns.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::Thread;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_split::align::AlignDelay;
use crate::runtime_state::BlockRuntimeNode;
use crate::worker_rt_policy;

/// How a path runs one node over its buffer.
pub(crate) type PathRun<'a> = dyn Fn(&mut BlockRuntimeNode, &mut [AudioFrame]) + Sync + 'a;

const IDLE: u8 = 0;
const READY: u8 = 1;
const DONE: u8 = 2;

/// One path's work for one callback. The pointers are valid while the
/// worker waits inside `run_paths`, which is the only time a lane reads them.
#[derive(Clone, Copy)]
struct Job {
    nodes: *mut Vec<BlockRuntimeNode>,
    buf: *mut Vec<AudioFrame>,
    align: *mut AlignDelay,
    run: *const PathRun<'static>,
    policy: Option<(u64, u64)>,
}

struct Lane {
    state: AtomicU8,
    stop: AtomicBool,
    started: AtomicBool,
    job: UnsafeCell<Option<Job>>,
    /// How many times the lane looked for work.
    #[cfg(test)]
    wakes: std::sync::atomic::AtomicU64,
}

// `job` is written only by the worker while `state` is IDLE, and read only
// by the lane while it is READY: the state hands it over.
unsafe impl Sync for Lane {}
unsafe impl Send for Lane {}

/// A lane and the thread that runs it.
struct LaneHandle {
    lane: Arc<Lane>,
    thread: Thread,
}

/// The lanes of one split: one per path but the first.
pub(crate) struct PathLanes {
    lanes: Vec<Option<LaneHandle>>,
}

impl PathLanes {
    /// Spawn a lane for every path of `paths` but the first. Build time only.
    pub(crate) fn spawn(paths: usize, name: &str) -> Self {
        let lanes = (1..paths).map(|index| spawn_lane(name, index)).collect();
        Self { lanes }
    }

    /// Run every path over its buffer and delay it into line: the first on
    /// the calling thread, the others on their lanes, all at once. Returns
    /// once every path is done.
    pub(crate) fn run_paths(
        &self,
        paths: &mut [Vec<BlockRuntimeNode>],
        bufs: &mut [Vec<AudioFrame>],
        aligns: &mut [AlignDelay],
        run: &PathRun<'_>,
    ) {
        let count = paths.len().min(bufs.len()).min(aligns.len());
        let (paths, bufs, aligns) = (paths.as_mut_ptr(), bufs.as_mut_ptr(), aligns.as_mut_ptr());
        // Erase the borrow's lifetime: the lanes are joined before return.
        let run: *const PathRun<'static> =
            unsafe { std::mem::transmute(run as *const PathRun<'_>) };
        let policy = worker_rt_policy::thread_policy();
        let mut handed = Handed {
            lanes: &self.lanes,
            count: 0,
        };
        for index in 1..count {
            let job = Job {
                nodes: unsafe { paths.add(index) },
                buf: unsafe { bufs.add(index) },
                align: unsafe { aligns.add(index) },
                run,
                policy,
            };
            match self.lanes.get(index - 1) {
                Some(Some(handle)) => {
                    unsafe { *handle.lane.job.get() = Some(job) };
                    handle.lane.state.store(READY, Ordering::Release);
                    handle.thread.unpark();
                }
                _ => unsafe { run_job(job) },
            }
            handed.count = index;
        }
        if count > 0 {
            unsafe {
                run_path(&mut *paths, &mut *bufs, &mut *aligns, &*run);
            }
        }
        drop(handed);
    }

    /// How many times the lanes looked for work, all together.
    #[cfg(test)]
    fn wakes(&self) -> u64 {
        self.lanes
            .iter()
            .flatten()
            .map(|handle| handle.lane.wakes.load(Ordering::Relaxed))
            .sum()
    }
}

impl Drop for PathLanes {
    fn drop(&mut self) {
        for handle in self.lanes.iter().flatten() {
            handle.lane.stop.store(true, Ordering::Release);
            handle.thread.unpark();
        }
    }
}

/// Waits, even on unwind, for every lane handed a job: the job points into
/// the split the caller is about to give back.
struct Handed<'a> {
    lanes: &'a [Option<LaneHandle>],
    count: usize,
}

impl Drop for Handed<'_> {
    fn drop(&mut self) {
        for handle in self.lanes.iter().take(self.count).flatten() {
            wait_done(&handle.lane);
            handle.lane.state.store(IDLE, Ordering::Relaxed);
        }
    }
}

/// Spin until the lane is done: see the module docs for why the worker
/// never parks here.
fn wait_done(lane: &Lane) {
    while lane.state.load(Ordering::Acquire) != DONE {
        std::hint::spin_loop();
    }
}

/// Run one path's nodes over its buffer, then delay it into line.
pub(crate) fn run_path(
    nodes: &mut [BlockRuntimeNode],
    buf: &mut Vec<AudioFrame>,
    align: &mut AlignDelay,
    run: &PathRun<'_>,
) {
    for node in nodes.iter_mut() {
        run(node, buf.as_mut_slice());
    }
    align.process(buf.as_mut_slice());
}

unsafe fn run_job(job: Job) {
    run_path(&mut *job.nodes, &mut *job.buf, &mut *job.align, &*job.run);
}

fn spawn_lane(name: &str, index: usize) -> Option<LaneHandle> {
    let lane = Arc::new(Lane {
        state: AtomicU8::new(IDLE),
        stop: AtomicBool::new(false),
        started: AtomicBool::new(false),
        job: UnsafeCell::new(None),
        #[cfg(test)]
        wakes: std::sync::atomic::AtomicU64::new(0),
    });
    let theirs = Arc::clone(&lane);
    let spawned = std::thread::Builder::new()
        .name(format!("split-path:{name}:{index}"))
        .spawn(move || lane_loop(&theirs));
    let thread = match spawned {
        Ok(joined) => joined.thread().clone(),
        Err(error) => {
            log::warn!("split '{name}': path {index} runs on the split's worker ({error})");
            return None;
        }
    };
    // A lane that has not started yet would stall the first callback.
    while !lane.started.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    Some(LaneHandle { lane, thread })
}

fn lane_loop(lane: &Lane) {
    lane.started.store(true, Ordering::Release);
    let mut adopted = None;
    loop {
        #[cfg(test)]
        lane.wakes.fetch_add(1, Ordering::Relaxed);
        if lane.stop.load(Ordering::Acquire) {
            return;
        }
        if lane.state.load(Ordering::Acquire) != READY {
            std::thread::park();
            continue;
        }
        if let Some(job) = unsafe { *lane.job.get() } {
            if job.policy != adopted {
                if let Some(policy) = job.policy {
                    worker_rt_policy::adopt(policy);
                }
                adopted = job.policy;
            }
            // A node that panics is caught and faulted inside `run`; this
            // only keeps a lane alive so the worker never waits forever.
            let _ =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { run_job(job) }));
        }
        lane.state.store(DONE, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "runtime_split_lanes_tests.rs"]
mod tests;
