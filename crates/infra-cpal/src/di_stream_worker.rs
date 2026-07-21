//! The `di-stream` render thread (#771) and its gapless hand-off (#785).
//!
//! One worker renders one armed DI: it builds the routed isolated runtime, then
//! streams it into a [`DiPlayback`] ring, paced by ring backpressure — the
//! output callback's consumption IS the clock. All DSP runs here, never in the
//! callback (invariant #8).
//!
//! A live edit does NOT tear the playback down. The incoming worker renders and
//! pre-rolls while the outgoing one keeps the listener supplied, seeks its loop
//! to where the listener will be, and takes the cell over exactly there — so an
//! edit costs neither a silent gap nor a restart of the take. Taking over also
//! stops EVERY worker it supersedes (edits can arrive faster than a render
//! builds, so more than one may be in flight) and retires the outgoing playback
//! off the audio thread.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use domain::io_binding::IoBinding;
use engine::di_render::build_routed_di_runtime;
use engine::runtime::{process_input_f32, process_output_f32};
use engine::DiPcm;
use project::chain::Chain;

use crate::di_playback::{DiPlayback, DiPlaybackCell, DiRetired};

/// Live `di-stream` render threads. A hand-off leaves the outgoing worker
/// running until the incoming one stops it; a worker left behind would burn a
/// core rendering a chain into a ring nobody plays. Pinned by the leak test.
pub(crate) static DI_WORKERS_ALIVE: AtomicUsize = AtomicUsize::new(0);


/// A stalled output (nothing consuming) would never reach the hand-off
/// position. After this long, the incoming render takes over anyway.
const HANDOFF_TIMEOUT: Duration = Duration::from_secs(2);

/// Frames rendered per worker iteration.
const BLOCK: usize = 256;

/// What a gapless re-arm hands to the incoming render: the playback the
/// listener is hearing, and the arm flags of every render thread still alive,
/// so the incoming one lines up with the playback and stops all of them.
pub(crate) struct DiHandoff {
    pub(crate) prev: Arc<DiPlayback>,
    pub(crate) prev_workers: Vec<Arc<Mutex<bool>>>,
}

/// Everything the worker needs that the controller resolved on the frontend.
pub(crate) struct DiWorkerSpec {
    pub(crate) chain: Chain,
    pub(crate) registry: Vec<IoBinding>,
    pub(crate) pcm: Arc<DiPcm>,
    pub(crate) output_rate: u32,
    pub(crate) dest_left: usize,
    pub(crate) dest_right: usize,
    pub(crate) cell: DiPlaybackCell,
    pub(crate) armed: Arc<Mutex<bool>>,
    pub(crate) failed: Arc<AtomicBool>,
    pub(crate) retired: DiRetired,
    pub(crate) handoff: Option<DiHandoff>,
    /// #808: the LIVE runtime the worker steps, published here after the initial
    /// build. A param edit rebuilds the routed runtime off-thread and swaps it in
    /// (wait-free) so the tone changes GAPLESSLY — no worker respawn, no output
    /// stream teardown (the "parou som"/"picotando"). The worker reads it every
    /// block, exactly as the guitar output callback reads its `LiveRuntimeSlot`.
    pub(crate) live_runtime: Arc<arc_swap::ArcSwapOption<engine::runtime::ChainRuntimeState>>,
    /// #808: the interface's output buffer (frames). The worker leads by a few
    /// of these — the DI buffers like a normal stream, not a hardcoded 32k ring.
    pub(crate) buffer_frames: u32,
}

/// Spawn the render thread for one armed DI.
pub(crate) fn spawn(spec: DiWorkerSpec) {
    std::thread::Builder::new()
        .name("di-stream".into())
        .spawn(move || {
            DI_WORKERS_ALIVE.fetch_add(1, Ordering::Relaxed);
            let _alive = AliveGuard;
            run(spec);
        })
        .expect("spawn di-stream thread");
}

/// Decrements [`DI_WORKERS_ALIVE`] on every exit path of a render thread.
struct AliveGuard;

impl Drop for AliveGuard {
    fn drop(&mut self) {
        DI_WORKERS_ALIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

fn run(spec: DiWorkerSpec) {
    let DiWorkerSpec {
        chain,
        registry,
        pcm,
        output_rate,
        dest_left,
        dest_right,
        cell,
        armed,
        failed,
        retired,
        handoff,
        live_runtime,
        buffer_frames,
    } = spec;

    // Build the routed isolated runtime (heavy: NAM/IR loads) OFF the frontend;
    // the loop is fed through the CHOSEN output's own binding (#716/#699 — a
    // flat render on a multi-binding chain was silent for any binding but the
    // first: the owner's "no sound").
    let routed = match build_routed_di_runtime(
        &chain,
        &registry,
        chain.di_output.as_ref(),
        output_rate,
        &pcm,
    ) {
        Ok(r) => r,
        Err(e) => {
            failed.store(true, Ordering::Relaxed);
            log::error!("di-stream build failed for chain '{}': {e:#}", chain.id.0);
            return;
        }
    };
    // Raw (pre-chain) loop for the DI IN meter.
    let raw = pcm.to_loop_at(output_rate);
    let raw_len = raw.len().max(1);

    // A cold arm parks after a ~100 ms pre-buffer, so playback starts with a
    // cushion instead of racing the worker from frame one (a 75 s loop still
    // starts in milliseconds — the full pre-render took minutes).
    //
    // A hand-off instead starts where the listener WILL be once the pre-roll is
    // ready, and waits to reach exactly that position before taking the cell.
    let loop_len = routed.loop_len.max(1);
    // #808: lead by the INTERFACE output buffer (a few of them). The DI buffers
    // like a normal stream — its pre-roll and lead are device-scaled, not a
    // hardcoded 32k ring — so a live runtime swap lands within a few buffers.
    let lead_frames = (buffer_frames as usize).max(64) * 4;
    let start_pos = handoff
        .as_ref()
        .map(|h| (h.prev.play_pos() + lead_frames) % loop_len)
        .unwrap_or(0);
    let playback = Arc::new(DiPlayback::starting_at(
        dest_left,
        dest_right,
        routed.loop_len,
        start_pos,
    ));
    routed.runtime.set_di_loop_pos(start_pos);
    // #808: publish the runtime the worker steps. A live param edit swaps a
    // freshly-built runtime in here; the loop below picks it up next block and
    // carries the loop position over, so the tone changes with no restart.
    live_runtime.store(Some(Arc::clone(&routed.runtime)));
    let mut active_rt = Arc::clone(&routed.runtime);
    let ring = playback.ring();
    let mut parked = false;
    let handoff_deadline = Instant::now() + HANDOFF_TIMEOUT;
    // Pre-buffer / lead, in ring SAMPLES (2 per frame): the interface buffer.
    let park_fill = lead_frames * 2;

    // Stream the DI: paced by RING BACKPRESSURE — the worker only produces what
    // the output consumed, so the output device clock IS the DI clock (no drift
    // by construction; the sleep-paced worker tried in #717 drifted and was
    // reverted, f1131725e).
    //
    // Scheduling shape matters more than raw priority (#698 lesson, re-measured
    // live on the owner's rig): normal priority + continuous burn → 71-88% fill
    // (preempted by the GUI + the guitar's RT worker); RT class + continuous
    // burn → 38% (the kernel demotes a time-constraint thread that blows
    // through its declared budget for seconds). The guitar's dsp_worker sustains
    // the SAME chain cost in debug because it works in SHORT BURSTS. Mirror it:
    // one block per iteration, a breath every few blocks, and an honest RT
    // declaration sized to that cadence.
    let period_ns = (BLOCK as u64) * 1_000_000_000 / (output_rate.max(1) as u64);
    crate::dsp_worker::promote_to_audio_rt(period_ns, period_ns * 3 / 5);
    let silence = vec![0.0f32; BLOCK];
    let mut drain = vec![0.0f32; BLOCK * routed.drain_width];
    let mut pos: usize = start_pos;
    // Catch-up bursts are capped: after a few back-to-back blocks the worker
    // yields, so it never presents the scheduler with a monolithic burn.
    let mut burst: u32 = 0;
    loop {
        if !still_armed(&armed) {
            return;
        }
        // Park BEFORE the backpressure check: a hand-off waits, with a full
        // ring, for the listener to reach the take-over position, so it must
        // still be evaluated while the worker is resting.
        if !parked && ring.len() >= park_fill {
            if !still_armed(&armed) {
                return;
            }
            match handoff.as_ref() {
                // Cold arm: nothing is sounding, park at once.
                None => {
                    cell.store(Some(Arc::clone(&playback)));
                    parked = true;
                }
                Some(h) => {
                    if handoff_reached(&h.prev, start_pos, loop_len)
                        || Instant::now() >= handoff_deadline
                    {
                        take_over(&cell, &playback, h, &retired);
                        parked = true;
                    }
                }
            }
        }
        // #808: lead by the interface buffer (park_fill), not the full ring — so
        // a live runtime swap is heard within a few buffers while the SPSC
        // backpressure still clocks the render.
        let target_fill = park_fill.min(ring.capacity() - BLOCK * 2);
        if ring.len() >= target_fill {
            burst = 0;
            std::thread::sleep(Duration::from_nanos(period_ns / 2));
            continue;
        }
        if burst >= 4 {
            burst = 0;
            std::thread::sleep(Duration::from_millis(1));
            continue;
        }
        burst += 1;
        // #808: pick up a live-swapped runtime (a param edit) and carry the loop
        // position onto it so the render stays continuous — no restart, gapless.
        if let Some(swapped) = live_runtime.load_full() {
            if !Arc::ptr_eq(&swapped, &active_rt) {
                swapped.set_di_loop_pos(active_rt.di_loop_pos());
                active_rt = swapped;
            }
        }
        process_input_f32(&active_rt, 0, &silence, 1);
        process_output_f32(
            &active_rt,
            routed.output_index,
            &mut drain,
            routed.drain_width,
        );
        for frame in drain.chunks(routed.drain_width) {
            let _ = ring.push(frame[routed.drain_left]);
            let _ = ring.push(frame[routed.drain_right]);
        }
        let mut in_peak = 0.0f32;
        for i in 0..BLOCK {
            let f = match raw.frame_at((pos + i) % raw_len) {
                engine::DiFrame::Mono(s) => s.abs(),
                engine::DiFrame::Stereo([a, b]) => a.abs().max(b.abs()),
            };
            in_peak = in_peak.max(f);
        }
        pos = (pos + BLOCK) % raw_len;
        playback.set_in_peak(in_peak);
    }
}

fn still_armed(armed: &Mutex<bool>) -> bool {
    *armed.lock().unwrap_or_else(|e| e.into_inner())
}

/// Swap this render into the output's cell: stop every worker it supersedes
/// (under their arm locks, so a parking race cannot resurrect one), publish the
/// new playback, and RETIRE the outgoing one — an in-flight callback may still
/// hold a guard on it, and the audio thread must never be the one to free a
/// multi-MB render buffer (invariant #8).
fn take_over(
    cell: &DiPlaybackCell,
    playback: &Arc<DiPlayback>,
    handoff: &DiHandoff,
    retired: &DiRetired,
) {
    let mut guards: Vec<_> = handoff
        .prev_workers
        .iter()
        .map(|w| w.lock().unwrap_or_else(|e| e.into_inner()))
        .collect();
    for armed in guards.iter_mut() {
        **armed = false;
    }
    cell.store(Some(Arc::clone(playback)));
    retired
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Arc::clone(&handoff.prev));
}

/// Has the outgoing playback reached the loop position the incoming render
/// starts from (within one block)? The tolerance also absorbs the callback
/// overshooting the exact frame — the position may land just PAST `start_pos`.
fn handoff_reached(prev: &DiPlayback, start_pos: usize, loop_len: usize) -> bool {
    let loop_len = loop_len.max(1);
    let ahead = (start_pos + loop_len - prev.play_pos() % loop_len) % loop_len;
    ahead <= BLOCK || ahead >= loop_len.saturating_sub(BLOCK)
}
