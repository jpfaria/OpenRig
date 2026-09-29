//! Responsibility: crossfades a live slot from the runtime a rebuild replaced to the one it published.
//!
//! #987: a live edit on a chain without a VST3 builds a whole new runtime off
//! the audio thread and publishes it into the chain's slot. The swap used to
//! be instant: the old runtime's output stopped on one sample and the new one
//! started from its primed cushion of silence and a fade-in from zero with cold
//! DSP — a step and a gap, the click on every scene switch. Now the old runtime
//! keeps playing while the new one warms up unheard (its cushion, its fade-in,
//! its cold blocks all behind it), then each output crossfades from the old to
//! the new. Both are versions of the SAME stream fed the same input, so nothing
//! crosses streams (invariant #4).
//!
//! Audio threads only load `Arc`s and add to atomics. The old runtime's last
//! reference is always dropped on the control side ([`SlotHandover::reap`]),
//! never on an audio thread.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arc_swap::ArcSwapOption;
use engine::runtime::{process_output_f32, ChainRuntimeState};
use engine::runtime_dsp::output_limiter;

use crate::LiveRuntimeSlot;

/// Output frames the new runtime plays unheard: its primed cushion (up to the
/// convolution cold-start cushion), its pipeline and block fades from silence,
/// and the first partitions of a cold IR or NAM.
pub(crate) const HANDOVER_WARMUP_FRAMES: usize = 1536;
/// Output frames of the raised-cosine crossfade that follows.
pub(crate) const HANDOVER_FADE_FRAMES: usize = 256;
/// Input frames the old runtime keeps being fed: the whole handover plus the
/// output side's lag behind the input (its cushion).
const HANDOVER_INPUT_FRAMES: usize = HANDOVER_WARMUP_FRAMES + HANDOVER_FADE_FRAMES + 4096;
/// Output routes a handover tracks; a route past it switches without one.
const HANDOVER_ROUTES: usize = 32;
/// How long the control side leaves the old runtime in the slot: well past the
/// handover at any rate and buffer the app runs.
const HANDOVER_HOLD: Duration = Duration::from_millis(300);

pub(crate) struct SlotHandover {
    outgoing: ArcSwapOption<ChainRuntimeState>,
    input_frames: AtomicUsize,
    route_frames: [AtomicUsize; HANDOVER_ROUTES],
    /// Control side only: when the running handover began.
    started: Mutex<Option<Instant>>,
    /// Control side only: old runtimes an audio thread may still hold.
    retired: Mutex<Vec<Arc<ChainRuntimeState>>>,
}

impl SlotHandover {
    pub(crate) fn new() -> Self {
        Self {
            outgoing: ArcSwapOption::empty(),
            input_frames: AtomicUsize::new(0),
            route_frames: std::array::from_fn(|_| AtomicUsize::new(0)),
            started: Mutex::new(None),
            retired: Mutex::new(Vec::new()),
        }
    }

    /// Control side: `previous` was just replaced in the slot; keep it playing
    /// until the new runtime has taken over.
    pub(crate) fn begin(&self, previous: Arc<ChainRuntimeState>) {
        self.input_frames.store(0, Ordering::Relaxed);
        for route in &self.route_frames {
            route.store(0, Ordering::Relaxed);
        }
        if let Some(displaced) = self.outgoing.swap(Some(previous)) {
            lock(&self.retired).push(displaced);
        }
        *lock(&self.started) = Some(Instant::now());
    }

    /// Control side: take a finished handover's runtime out of the slot and
    /// hand back every old runtime no audio thread holds any more, for the
    /// caller to drop off the audio thread.
    pub(crate) fn reap(&self, now: Instant) -> Vec<Arc<ChainRuntimeState>> {
        {
            let mut started = lock(&self.started);
            if started.is_some_and(|at| now.duration_since(at) >= HANDOVER_HOLD) {
                if let Some(old) = self.outgoing.swap(None) {
                    lock(&self.retired).push(old);
                }
                *started = None;
            }
        }
        let mut retired = lock(&self.retired);
        let (free, held): (Vec<_>, Vec<_>) = retired
            .drain(..)
            .partition(|old| Arc::strong_count(old) == 1);
        *retired = held;
        free
    }

    /// Input side: the old runtime, while it still has to be fed.
    pub(crate) fn outgoing_for_input(&self, frames: usize) -> Option<Arc<ChainRuntimeState>> {
        let old = self.outgoing.load_full()?;
        let fed = self.input_frames.fetch_add(frames, Ordering::Relaxed);
        (fed < HANDOVER_INPUT_FRAMES).then_some(old)
    }

    /// Output side: the old runtime and how far `route` is into the handover,
    /// while the route still plays it.
    fn outgoing_for_route(&self, route: usize) -> Option<(Arc<ChainRuntimeState>, usize)> {
        let played = self.route_frames.get(route)?.load(Ordering::Relaxed);
        if played >= HANDOVER_WARMUP_FRAMES + HANDOVER_FADE_FRAMES {
            return None;
        }
        Some((self.outgoing.load_full()?, played))
    }

    fn advance_route(&self, route: usize, frames: usize) {
        if let Some(played) = self.route_frames.get(route) {
            played.fetch_add(frames, Ordering::Relaxed);
        }
    }

    pub(crate) fn plays_on(&self, route: usize) -> bool {
        self.outgoing_for_route(route).is_some()
    }
}

/// The new runtime's gain `played` frames into a handover.
pub(crate) fn new_runtime_gain(played: usize) -> f32 {
    if played < HANDOVER_WARMUP_FRAMES {
        return 0.0;
    }
    let into_fade = played - HANDOVER_WARMUP_FRAMES;
    if into_fade >= HANDOVER_FADE_FRAMES {
        return 1.0;
    }
    let progress = into_fade as f32 / HANDOVER_FADE_FRAMES as f32;
    0.5 * (1.0 - (std::f32::consts::PI * progress).cos())
}

/// Output side: mix `slots` into `out` like `process_output_f32_mixed`, each
/// slot in a handover crossfading its old runtime into its current one.
/// `loaded` holds the slots' current runtimes, in order.
pub(crate) fn mix_with_handover(
    slots: &[LiveRuntimeSlot],
    loaded: &[Arc<ChainRuntimeState>],
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    out.fill(0.0);
    let n = out.len();
    let channels = output_total_channels.max(1);
    let buf = &mut scratch[..n];
    for (slot, current) in slots.iter().zip(loaded.iter()) {
        process_output_f32(current, output_index, buf, output_total_channels);
        let Some((old, played)) = slot.handover().outgoing_for_route(output_index) else {
            for (dst, src) in out.iter_mut().zip(buf.iter()) {
                *dst += *src;
            }
            continue;
        };
        for (j, (dst, src)) in out.iter_mut().zip(buf.iter()).enumerate() {
            *dst += new_runtime_gain(played + j / channels) * *src;
        }
        process_output_f32(&old, output_index, buf, output_total_channels);
        for (j, (dst, src)) in out.iter_mut().zip(buf.iter()).enumerate() {
            *dst += (1.0 - new_runtime_gain(played + j / channels)) * *src;
        }
        slot.handover().advance_route(output_index, n / channels);
    }
    if slots.len() > 1 {
        for s in out.iter_mut() {
            *s = output_limiter(*s);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
