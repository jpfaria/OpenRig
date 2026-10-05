//! A split's paths run on threads of their own, at the same time, and the
//! threads take the realtime policy of the worker that drives the split.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use block_core::MonoProcessor;
use crossbeam_queue::ArrayQueue;
use domain::ids::BlockId;
use project::block::split_params::default_split_params;

use crate::runtime_audio_frame::AudioFrame;
use crate::runtime_process_segment::process_audio_block;
use crate::runtime_split::knobs::SplitKnobs;
use crate::runtime_split::process::process_split;
use crate::runtime_split::state::SplitRuntimeState;
use crate::runtime_split::test_support::mono_node;
use crate::runtime_state::{BlockError, BlockRuntimeNode};
use crate::worker_rt_policy;

/// Records every thread it runs on.
struct ThreadProbe(Arc<Mutex<Vec<ThreadId>>>);

impl MonoProcessor for ThreadProbe {
    fn process_sample(&mut self, input: f32) -> f32 {
        let id = std::thread::current().id();
        let mut seen = self.0.lock().unwrap();
        if !seen.contains(&id) {
            seen.push(id);
        }
        input
    }
}

/// On its first sample, waits until every path has arrived — which only
/// happens when the paths run at the same time.
struct Rendezvous {
    arrived: Arc<AtomicUsize>,
    met: Arc<AtomicUsize>,
    expected: usize,
    done: bool,
}

impl MonoProcessor for Rendezvous {
    fn process_sample(&mut self, input: f32) -> f32 {
        if !self.done {
            self.done = true;
            self.arrived.fetch_add(1, Ordering::SeqCst);
            let start = Instant::now();
            while self.arrived.load(Ordering::SeqCst) < self.expected
                && start.elapsed() < Duration::from_millis(500)
            {
                std::hint::spin_loop();
            }
            if self.arrived.load(Ordering::SeqCst) >= self.expected {
                self.met.fetch_add(1, Ordering::SeqCst);
            }
        }
        input
    }
}

fn split(paths: Vec<Vec<BlockRuntimeNode>>) -> SplitRuntimeState {
    let knobs = SplitKnobs::from_params(&default_split_params(paths.len()), paths.len());
    SplitRuntimeState::new(true, paths, knobs, &BlockId("split".into()))
}

fn run(state: &mut SplitRuntimeState, frames: usize) {
    let queue = ArrayQueue::<BlockError>::new(8);
    let mut buf = vec![AudioFrame::Stereo([0.1, 0.1]); frames];
    process_split(state, &mut buf, |node, path| {
        process_audio_block(node, path, &queue)
    });
}

#[test]
fn every_path_runs_on_a_thread_of_its_own() {
    let seen: Vec<_> = (0..3).map(|_| Arc::new(Mutex::new(Vec::new()))).collect();
    let paths = seen
        .iter()
        .enumerate()
        .map(|(i, s)| {
            vec![mono_node(
                &format!("p{i}"),
                Box::new(ThreadProbe(s.clone())),
            )]
        })
        .collect();
    let mut state = split(paths);
    run(&mut state, 64);
    run(&mut state, 64);
    let threads: Vec<Vec<ThreadId>> = seen.iter().map(|s| s.lock().unwrap().clone()).collect();
    for (i, a) in threads.iter().enumerate() {
        assert_eq!(a.len(), 1, "path {i} ran on more than one thread: {a:?}");
        for b in threads.iter().skip(i + 1) {
            assert_ne!(a[0], b[0], "two paths shared one thread");
        }
    }
}

#[test]
fn the_paths_run_at_the_same_time() {
    let arrived = Arc::new(AtomicUsize::new(0));
    let met = Arc::new(AtomicUsize::new(0));
    let paths = (0..3)
        .map(|i| {
            vec![mono_node(
                &format!("p{i}"),
                Box::new(Rendezvous {
                    arrived: arrived.clone(),
                    met: met.clone(),
                    expected: 3,
                    done: false,
                }),
            )]
        })
        .collect();
    let mut state = split(paths);
    run(&mut state, 64);
    assert_eq!(
        met.load(Ordering::SeqCst),
        3,
        "the paths ran one after another"
    );
}

static PROMOTED: Mutex<Vec<(u64, u64)>> = Mutex::new(Vec::new());

fn record_promotion(period_ns: u64, computation_ns: u64) {
    PROMOTED.lock().unwrap().push((period_ns, computation_ns));
}

#[test]
fn path_threads_take_the_realtime_policy_of_the_worker_driving_the_split() {
    worker_rt_policy::set_promoter(record_promotion);
    worker_rt_policy::set_thread_policy(1_333_111, 999_777);
    let paths = (0..2)
        .map(|i| {
            vec![mono_node(
                &format!("p{i}"),
                Box::new(ThreadProbe(Default::default())),
            )]
        })
        .collect();
    let mut state = split(paths);
    run(&mut state, 64);
    assert!(
        PROMOTED.lock().unwrap().contains(&(1_333_111, 999_777)),
        "no path thread was promoted to the worker's realtime policy"
    );
}
