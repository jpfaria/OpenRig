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
#[derive(Default)]
pub struct ChainMeterRings {
    pub input: Vec<Arc<SpscRing<f32>>>,
    pub output: Vec<Arc<SpscRing<f32>>>,
}

/// Shared store of every subscribed chain's meter rings. Cheap
/// `Rc<RefCell<HashMap>>` because both the timer and the chain-
/// lifecycle code mutate it from the GUI thread.
pub type MeterStore =
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<domain::ids::ChainId, ChainMeterRings>>>;

pub fn new_meter_store() -> MeterStore {
    std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()))
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

    let store = new_meter_store();
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
        ensure_subscribed(controller, &store, &chain_ids, RING_CAPACITY);
        prune_dead(&store, &chain_ids);
        // Re-subscribing every tick (see `refresh_subscriptions`) means
        // each chain's previous SPSC ring Arcs are dropped on insert.
        // Tell the runtime to sweep its tap registry so the now-orphan
        // consumer slots are reclaimed; otherwise a long-running session
        // would accumulate ~30 dead tap entries per chain per second.
        controller.prune_dead_input_taps();
        controller.prune_dead_stream_taps();
        let readings = poll_all(&store);
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
