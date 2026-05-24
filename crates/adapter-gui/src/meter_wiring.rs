//! Per-chain IN/OUT dBFS meter wiring — issue #496 / #32 / #36.
//!
//! Lifecycle:
//! 1. On chain create/upsert, subscribe to the chain's input_tap
//!    (channel 0) and stream_tap (stream 0) via the
//!    `ProjectRuntimeController`. Store the returned SPSC ring
//!    handles in `MeterState::chains` keyed by `ChainId`.
//! 2. A Slint `Timer` running at ~30 Hz calls
//!    [`compute_meter_for_chain`] for each subscribed chain, then
//!    writes the resulting dBFS pair into the matching
//!    `ProjectChainItem` row of the `project_chains` VecModel.
//! 3. Dropped chains are pruned by `prune_dead_*_taps` on the
//!    controller (existing infra).
//!
//! Only the pure compute function is exposed at the moment so it can
//! be unit-tested without spinning up a Slint runtime or an engine
//! runtime. The Slint Timer + subscribe glue will follow once the
//! pure layer is locked in.

use std::sync::Arc;

use engine::output_meter::{pop_peak_dbfs, SILENT_DBFS};
use engine::spsc::SpscRing;

/// Apply the chain's volume slider (in percent, 100 = unity) to a
/// raw peak-dBFS reading. The stream_tap is captured BEFORE the
/// audio callback scales by `volume_pct/100`, so the OUTPUT meter
/// has to add `20·log10(volume_pct/100)` on the GUI side to reflect
/// what actually reaches the DAC. `SILENT_DBFS` round-trips
/// unchanged; a 0 % volume is treated as silence.
pub fn apply_chain_volume_db(base_dbfs: f32, volume_pct: f32) -> f32 {
    if base_dbfs <= SILENT_DBFS + 1.0 {
        return SILENT_DBFS;
    }
    if volume_pct <= 0.0 {
        return SILENT_DBFS;
    }
    base_dbfs + 20.0 * (volume_pct / 100.0).log10()
}

/// Drain the current windows of a chain's input and output taps and
/// return `(input_peak_dbfs, output_peak_dbfs)`. Either side reports
/// [`SILENT_DBFS`] when its rings are empty.
///
/// Pure over the supplied rings — no Slint, no engine runtime,
/// directly testable.
pub fn compute_meter_for_chain(
    input_rings: &[Arc<SpscRing<f32>>],
    output_rings: &[Arc<SpscRing<f32>>],
) -> (f32, f32) {
    let i = if input_rings.is_empty() {
        SILENT_DBFS
    } else {
        pop_peak_dbfs(input_rings)
    };
    let o = if output_rings.is_empty() {
        SILENT_DBFS
    } else {
        pop_peak_dbfs(output_rings)
    };
    (i, o)
}

/// Per-chain ring handles held by the GUI so the meter timer can
/// drain them without re-subscribing every tick.
#[derive(Default, Clone)]
pub struct ChainMeterRings {
    pub input: Vec<Arc<SpscRing<f32>>>,
    pub output: Vec<Arc<SpscRing<f32>>>,
}

/// One stream's worth of meter rings (input + output). A chain owns
/// N streams (multi-input layouts); the per-stream meter layer keeps
/// one entry per stream so each one is visible in the GUI instead of
/// only the first.
#[derive(Default, Clone)]
pub struct StreamMeterRings {
    pub input: Vec<Arc<SpscRing<f32>>>,
    pub output: Vec<Arc<SpscRing<f32>>>,
}

/// All meter rings for one chain, indexed by stream order. The list
/// length equals `controller.stream_count(chain_id)` at subscribe time.
#[derive(Default, Clone)]
pub struct ChainMeterStreams {
    pub streams: Vec<StreamMeterRings>,
}

impl ChainMeterStreams {
    /// Backwards-compat collapse to the legacy single-pair shape
    /// (first stream's rings, or empty when the chain has no streams).
    pub fn first_stream_or_default(&self) -> ChainMeterRings {
        match self.streams.first() {
            Some(s) => ChainMeterRings {
                input: s.input.clone(),
                output: s.output.clone(),
            },
            None => ChainMeterRings::default(),
        }
    }
}

/// Per-stream peak readings for one chain, returned by `poll_per_stream`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamMeterReading {
    pub in_dbfs: f32,
    pub out_dbfs: f32,
}

/// Shared store of every subscribed chain's meter rings. Cheap
/// `Rc<RefCell<HashMap>>` because both the timer and the chain-
/// lifecycle code mutate it from the GUI thread.
pub type MeterStore =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<domain::ids::ChainId, ChainMeterRings>>>;

pub fn new_meter_store() -> MeterStore {
    std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()))
}

/// Per-stream meter store: each chain id maps to a list of stream
/// meter rings (one entry per stream the runtime exposes).
pub type MeterStorePerStream = std::rc::Rc<
    std::cell::RefCell<std::collections::HashMap<domain::ids::ChainId, ChainMeterStreams>>,
>;

pub fn new_meter_store_per_stream() -> MeterStorePerStream {
    std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()))
}

/// Per-stream variant of `refresh_subscriptions`: drops entries for
/// chains no longer present and re-subscribes the rest on every call.
/// Eager subscribe path — used in tests that pin the every-tick
/// re-subscribe property. Production timers should call the lazy
/// variant `refresh_subscriptions_lazy_per_stream`, which keeps the
/// rings stable across ticks (otherwise the meter visibly flickers
/// at ~30 Hz as each fresh ring starts empty).
pub fn refresh_subscriptions_per_stream<F>(
    store: &MeterStorePerStream,
    chain_ids: &[domain::ids::ChainId],
    make_streams: &F,
) where
    F: Fn(&domain::ids::ChainId) -> ChainMeterStreams,
{
    let mut store = store.borrow_mut();
    store.retain(|cid, _| chain_ids.contains(cid));
    for cid in chain_ids {
        store.insert(cid.clone(), make_streams(cid));
    }
}

/// Production-friendly per-stream refresh: keep existing entries
/// untouched (no flicker), drop entries for chains no longer present,
/// re-subscribe only when explicitly invalidated by the caller
/// (toggle enabled, rig-nav, runtime restart). Pass `invalidate=[]`
/// for the steady-state tick.
pub fn refresh_subscriptions_lazy_per_stream<F>(
    store: &MeterStorePerStream,
    chain_ids: &[domain::ids::ChainId],
    invalidate: &[domain::ids::ChainId],
    make_streams: &F,
) where
    F: Fn(&domain::ids::ChainId) -> ChainMeterStreams,
{
    let mut store = store.borrow_mut();
    store.retain(|cid, _| chain_ids.contains(cid));
    for cid in invalidate {
        store.remove(cid);
    }
    for cid in chain_ids {
        if !store.contains_key(cid) {
            store.insert(cid.clone(), make_streams(cid));
        }
    }
}

/// Drain the per-stream rings and return one `StreamMeterReading`
/// per stream for every chain in the store.
pub fn poll_per_stream(
    store: &MeterStorePerStream,
) -> Vec<(domain::ids::ChainId, Vec<StreamMeterReading>)> {
    let store = store.borrow();
    store
        .iter()
        .map(|(cid, streams)| {
            let readings = streams
                .streams
                .iter()
                .map(|s| {
                    let (i, o) = compute_meter_for_chain(&s.input, &s.output);
                    StreamMeterReading {
                        in_dbfs: i,
                        out_dbfs: o,
                    }
                })
                .collect();
            (cid.clone(), readings)
        })
        .collect()
}

/// Make sure every chain currently in `chain_ids` has its meter taps
/// subscribed. Dropped chains stay in the store until they are
/// removed by [`prune_dead`].
///
/// Wrapper around [`refresh_subscriptions`] that calls the real
/// controller to materialise the rings. Kept for backward compatibility
/// of any external caller; new code should prefer
/// [`refresh_subscriptions`] which is closure-injected and testable.
pub fn ensure_subscribed(
    controller: &infra_cpal::ProjectRuntimeController,
    store: &MeterStore,
    chain_ids: &[domain::ids::ChainId],
    capacity_per_channel: usize,
) {
    let make_rings = |cid: &domain::ids::ChainId| -> ChainMeterRings {
        let input = controller.subscribe_input_tap(cid, 0, 2, &[0, 1], capacity_per_channel);
        let output = controller
            .subscribe_stream_tap(cid, 0, capacity_per_channel)
            .map(|[l, r]| vec![l, r])
            .unwrap_or_default();
        ChainMeterRings { input, output }
    };
    refresh_subscriptions(store, chain_ids, &make_rings);
}

/// Closure-injected core of the meter subscription lifecycle.
///
/// (1) Calls `make_rings(cid)` for **every** chain in `chain_ids` on
///     every tick (replacing any previous entry). This is the fix for
///     the user-reported regression where toggling a chain or switching
///     preset froze the meter at -∞ dBFS: the runtime swap left the
///     old SPSC rings dangling, and the old "skip if present" guard
///     never replaced them.
/// (2) Drops entries for chains no longer in `chain_ids`.
///
/// Pure over the closure; unit-testable without a real controller.
pub fn refresh_subscriptions<F>(
    store: &MeterStore,
    chain_ids: &[domain::ids::ChainId],
    make_rings: &F,
) where
    F: Fn(&domain::ids::ChainId) -> ChainMeterRings,
{
    let mut store = store.borrow_mut();
    store.retain(|cid, _| chain_ids.contains(cid));
    for cid in chain_ids {
        let rings = make_rings(cid);
        store.insert(cid.clone(), rings);
    }
}

/// Drop entries for chains no longer in `chain_ids`. The underlying
/// engine-side taps are pruned by their own dead-consumer sweep
/// (`prune_dead_input_taps` / `prune_dead_stream_taps`) once the
/// last Arc here is dropped.
pub fn prune_dead(store: &MeterStore, chain_ids: &[domain::ids::ChainId]) {
    let mut store = store.borrow_mut();
    store.retain(|cid, _| chain_ids.contains(cid));
}

/// Pull the current peak dBFS for every subscribed chain. Returns
/// a list of `(chain_id, in_dbfs, out_dbfs)` for the timer to fan
/// out into the Slint VecModel.
pub fn poll_all(store: &MeterStore) -> Vec<(domain::ids::ChainId, f32, f32)> {
    let store = store.borrow();
    store
        .iter()
        .map(|(cid, rings)| {
            let (i, o) = compute_meter_for_chain(&rings.input, &rings.output);
            (cid.clone(), i, o)
        })
        .collect()
}

/// Lifecycle wiring: starts a Slint Timer that, at ~30 Hz, picks up
/// the current chain list from the project session, ensures every
/// chain has its meter taps subscribed, polls them, and writes the
/// per-chain peak dBFS into the matching `ProjectChainItem` rows of
/// the `project_chains` VecModel. Timer is leaked (lives for the
/// app's lifetime, like the other polling timers).
pub fn start_meter_polling(
    project_runtime: std::rc::Rc<std::cell::RefCell<Option<infra_cpal::ProjectRuntimeController>>>,
    project_chains: std::rc::Rc<slint::VecModel<crate::ProjectChainItem>>,
    project_session: std::rc::Rc<std::cell::RefCell<Option<crate::state::ProjectSession>>>,
) {
    use slint::{Model, TimerMode};
    const TICK: std::time::Duration = std::time::Duration::from_millis(33); // ~30 Hz
    const RING_CAPACITY: usize = 4096; // 30 Hz poll @ 48 kHz ⇒ 1600 samples per drain

    let store = new_meter_store_per_stream();
    // Per-chain enabled snapshot from the previous tick. A chain whose
    // enabled flag flipped is invalidated so the meter re-subscribes
    // to the freshly-started runtime; chains whose state didn't change
    // keep their stable ring handles (no re-subscription ⇒ no flicker).
    let last_enabled: std::rc::Rc<
        std::cell::RefCell<std::collections::HashMap<domain::ids::ChainId, bool>>,
    > = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()));
    let timer = slint::Timer::default();
    timer.start(TimerMode::Repeated, TICK, move || {
        let rt_borrow = project_runtime.borrow();
        let Some(controller) = rt_borrow.as_ref() else {
            return;
        };
        let session_borrow = project_session.borrow();
        let Some(session) = session_borrow.as_ref() else {
            return;
        };
        let project = session.project.borrow();
        let chain_ids: Vec<_> = project.chains.iter().map(|c| c.id.clone()).collect();
        // Detect chains whose enabled flag flipped since the last tick.
        // Those (and only those) need their meter rings refreshed.
        let mut invalidate: Vec<domain::ids::ChainId> = Vec::new();
        {
            let mut last = last_enabled.borrow_mut();
            for c in project.chains.iter() {
                let prev = last.get(&c.id).copied();
                if prev != Some(c.enabled) {
                    invalidate.push(c.id.clone());
                    last.insert(c.id.clone(), c.enabled);
                }
            }
            last.retain(|cid, _| chain_ids.contains(cid));
        }
        let make_streams = |cid: &domain::ids::ChainId| -> ChainMeterStreams {
            let n = controller.stream_count(cid);
            let streams = (0..n)
                .map(|i| StreamMeterRings {
                    input: controller.subscribe_input_tap(cid, i, 2, &[0, 1], RING_CAPACITY),
                    output: controller
                        .subscribe_stream_tap(cid, i, RING_CAPACITY)
                        .map(|[l, r]| vec![l, r])
                        .unwrap_or_default(),
                })
                .collect();
            ChainMeterStreams { streams }
        };
        refresh_subscriptions_lazy_per_stream(&store, &chain_ids, &invalidate, &make_streams);
        // Reclaim any orphan tap slots left behind after an invalidation
        // (rings dropped from the store free their consumer side, the
        // runtime sweeps).
        if !invalidate.is_empty() {
            controller.prune_dead_input_taps();
            controller.prune_dead_stream_taps();
        }
        // Aggregate per-stream readings to a single (max in, max out)
        // pair per chain so the existing single-bar UI keeps showing
        // *something*. The per-stream values are also forwarded into
        // `ProjectChainItem.stream_meters` for the multi-stream UI
        // surface.
        let per_stream = poll_per_stream(&store);
        let readings: Vec<(domain::ids::ChainId, f32, f32)> = per_stream
            .iter()
            .map(|(cid, streams)| {
                let max_in = streams
                    .iter()
                    .map(|s| s.in_dbfs)
                    .fold(engine::output_meter::SILENT_DBFS, f32::max);
                let max_out = streams
                    .iter()
                    .map(|s| s.out_dbfs)
                    .fold(engine::output_meter::SILENT_DBFS, f32::max);
                (cid.clone(), max_in, max_out)
            })
            .collect();
        // Push readings into matching VecModel rows (rows are indexed
        // 1:1 with `project.chains`). The stream_tap reads the chain
        // signal BEFORE the audio callback applies the chain volume
        // slider — so the OUTPUT meter must compensate on the GUI
        // side, otherwise moving the volume knob never changes the
        // reading (user-reported in issue #496).
        for (cid, in_db, out_db_raw) in readings {
            let Some(idx) = project.chains.iter().position(|c| c.id == cid) else {
                continue;
            };
            let Some(mut row) = project_chains.row_data(idx) else {
                continue;
            };
            let out_db = apply_chain_volume_db(out_db_raw, project.chains[idx].volume);
            if (row.meter_in_dbfs - in_db).abs() > 0.05
                || (row.meter_out_dbfs - out_db).abs() > 0.05
            {
                row.meter_in_dbfs = in_db;
                row.meter_out_dbfs = out_db;
                project_chains.set_row_data(idx, row);
            }
        }
    });
    std::mem::forget(timer);
}

#[cfg(test)]
#[path = "meter_wiring_tests.rs"]
mod tests;
