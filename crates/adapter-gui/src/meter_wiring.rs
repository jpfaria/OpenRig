//! Per-chain IN/OUT dBFS meter wiring — issue #496 / #32 / #36.
//!
//! Lifecycle:
//! 1. On chain create/upsert, subscribe to the chain's input tap
//!    (channel 0) and stream tap (stream 0) through the subscription
//!    seam (`application::audio_taps::AudioTaps`), by the identity of
//!    the stream. Store the returned tap handles in the per-stream
//!    meter store keyed by `ChainId`.
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

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use engine::output_meter::SILENT_DBFS;

// Issue #792: the meter-polling timer + its per-chain row refresh live in
// meter_wiring_poll.rs (decomposed, not just relocated). Re-exported so
// crate::meter_wiring::start_meter_polling (called from desktop_app) resolves.
pub use crate::meter_wiring_poll::start_meter_polling;

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

/// Poll one stream's input and output subscriptions and return
/// `(input_peak_dbfs, output_peak_dbfs)`. Either side reports
/// [`SILENT_DBFS`] when it has no subscription — a stream that is not
/// there to tap is silent, never a fabricated level.
///
/// Pure over the supplied taps — no Slint, no engine runtime,
/// directly testable. The reduction itself lives behind the seam
/// ([`AudioTap::poll_peak_dbfs`]), so a frontend that carries only
/// finished readings reports the same numbers.
pub fn compute_meter_for_chain(
    input: Option<&Arc<dyn AudioTap>>,
    output: Option<&Arc<dyn AudioTap>>,
) -> (f32, f32) {
    (
        input.map_or(SILENT_DBFS, |tap| tap.poll_peak_dbfs()),
        output.map_or(SILENT_DBFS, |tap| tap.poll_peak_dbfs()),
    )
}

/// One stream's worth of meter subscriptions (input + output). A chain
/// owns N streams (multi-input layouts); the per-stream meter layer
/// keeps one entry per stream so each one is visible in the GUI instead
/// of only the first.
#[derive(Default, Clone)]
pub struct StreamMeterTaps {
    pub input: Option<Arc<dyn AudioTap>>,
    pub output: Option<Arc<dyn AudioTap>>,
}

/// All meter subscriptions for one chain, indexed by stream order. The
/// list length equals `taps.stream_count(chain_id)` at subscribe time.
#[derive(Default, Clone)]
pub struct ChainMeterStreams {
    pub streams: Vec<StreamMeterTaps>,
}

/// Per-stream peak readings for one chain, returned by `poll_per_stream`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamMeterReading {
    pub in_dbfs: f32,
    pub out_dbfs: f32,
}

/// Per-stream meter store: each chain id maps to a list of stream
/// meter rings (one entry per stream the runtime exposes).
pub type MeterStorePerStream = std::rc::Rc<
    std::cell::RefCell<std::collections::HashMap<domain::ids::ChainId, ChainMeterStreams>>,
>;

pub fn new_meter_store_per_stream() -> MeterStorePerStream {
    std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::new()))
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

/// Walk the project's chains, compute each one's current
/// `timer_chain_signature`, compare against the cached value, and
/// return the list of chains whose signature changed (must be
/// re-subscribed).
///
/// Nothing hosted ([`AudioTaps::is_hosted`] false) means the runtime is
/// offline. The whole cache is wiped so the next online tick treats
/// every chain as a fresh subscription — without this, toggling off the
/// last enabled chain drops the runtime, and the subsequent toggle-on
/// (which spins up a NEW one with the same project state) would produce
/// the same cached signature, skip invalidation, and leave the meter
/// store handing out taps opened against the dropped runtime.
pub fn detect_invalidations(
    chains: &[project::chain::Chain],
    taps: &dyn AudioTaps,
    last_signature: &mut std::collections::HashMap<domain::ids::ChainId, u64>,
) -> Vec<domain::ids::ChainId> {
    let chain_ids: Vec<_> = chains.iter().map(|c| c.id.clone()).collect();
    if !taps.is_hosted() {
        last_signature.clear();
        return Vec::new();
    }
    let mut invalidate = Vec::new();
    for c in chains.iter() {
        let sig = timer_chain_signature(c, taps.stream_count(&c.id));
        if last_signature.get(&c.id).copied() != Some(sig) {
            invalidate.push(c.id.clone());
            last_signature.insert(c.id.clone(), sig);
        }
    }
    last_signature.retain(|cid, _| chain_ids.contains(cid));
    invalidate
}

/// Full per-tick "did anything that requires a re-subscribe change?"
/// signature: project-side bits AND the engine's current stream
/// count for this chain. Stream count is the SUM across this chain's
/// per-input runtimes (issue #350) and drops to 0 when the engine
/// tears them down (chain toggle off, rig-nav rebuild, device
/// reopen). Folding it into the signature is what makes the timer
/// invalidate the dead ring handles a teardown leaves behind —
/// `chain.enabled` alone is not enough because the project state and
/// the engine state can disagree during the rebuild window. Hashes
/// `(chain_meter_signature, stream_count)` together so neither
/// dimension can mask a change in the other.
pub fn timer_chain_signature(chain: &project::chain::Chain, stream_count: usize) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    chain_meter_signature(chain).hash(&mut h);
    stream_count.hash(&mut h);
    h.finish()
}

/// Compact "did the runtime layout change?" signature for a chain.
/// Includes the chain's enabled flag and every block's `(id, enabled)`
/// — the bits that flip when the runtime is torn down and rebuilt
/// (toggle, rig-nav preset/scene switch, block add/remove). NOT
/// affected by knob/param value changes, so steady-state ticks don't
/// cause a re-subscribe (that's the flicker fix). The meter timer
/// compares the signature against the previous tick's value and
/// invalidates the chain's meter store entry on any difference.
pub fn chain_meter_signature(chain: &project::chain::Chain) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    chain.enabled.hash(&mut h);
    for b in &chain.blocks {
        b.id.0.hash(&mut h);
        b.enabled.hash(&mut h);
    }
    h.finish()
}

/// Issue #670: a chain is "overloading" when its audio callback counted
/// MORE deadline misses (xruns) than the previous meter poll saw — i.e.
/// the user is hearing dropouts right now. The timer keeps the previous
/// per-chain count; a decrease means the counter was reset (e.g. on a
/// chain rebuild), not a fresh overrun.
pub(crate) fn chain_overloaded(prev_xruns: u64, cur_xruns: u64) -> bool {
    cur_xruns > prev_xruns
}

/// Build the per-stream meter rings for a chain by asking the runtime
/// how many streams it actually owns and subscribing each one
/// independently.
///
/// History — replaces the older "subscribe channels 0..N of runtime
/// 0 once, broadcast the same output ring across rows" path
/// (silenced rows 1..N because `SpscRing` is single-consumer) and the
/// follow-up "`subscribe_input_tap(cid, i, 1, &[0], cap)`" pattern
/// that issue #557 finally killed: that one was wrong on two counts —
/// the global stream index was used as the runtime-side `input_index`
/// filter (silencing any tap past index 0 on same-device multi-stream
/// chains), and `&[0]` ignored the chain's actual input endpoint
/// channels (the meter for a chain wired to device channel 1 ended up
/// reading channel 0 — the wrong guitar).
///
/// Now each row subscribes by IDENTITY through the seam:
/// [`TapPoint::StreamInput`] (the implementation resolves the per-input
/// runtime, the cpal group, and the endpoint channel) and
/// [`TapPoint::StreamOutput`] (per-stream stereo post-FX, unchanged —
/// its dispatch already translates global stream index to local
/// segment).
pub fn build_streams_from_taps(
    taps: &dyn AudioTaps,
    chain_id: &domain::ids::ChainId,
    capacity_per_channel: usize,
) -> ChainMeterStreams {
    let stream_count = taps.stream_count(chain_id);
    let streams = (0..stream_count)
        .map(|i| StreamMeterTaps {
            input: taps.subscribe(
                &TapPoint::StreamInput {
                    chain: chain_id.clone(),
                    stream: i,
                },
                capacity_per_channel,
            ),
            output: taps.subscribe(
                &TapPoint::StreamOutput {
                    chain: chain_id.clone(),
                    stream: i,
                },
                capacity_per_channel,
            ),
        })
        .collect();
    ChainMeterStreams { streams }
}

/// Build the per-chain `stream_meters` row payload the GUI must show.
///
/// Issue #532: the row length is owned by the project state — one
/// slot per input entry in the chain (with a min of 1 mirroring
/// `replace_project_chains`'s `.max(1)` clamp) — NOT by the engine's
/// transient per-tick stream count. If the engine reports more streams
/// than the project owns (transient mid-rebuild after a preset switch),
/// the extra readings are dropped. If it reports fewer (sibling chain
/// re-spawning after a toggle), the missing slots stay [`SILENT_DBFS`].
/// Both symptoms reported in #532 collapse to the same fix.
///
/// Issue #750: when `enabled` is false the row is EMPTY — the per-stream graph
/// is a live surface that must not show on a disabled chain. This overrides the
/// `.max(1)` clamp, so the timer can't re-grow the footer a tick after the
/// chain is switched off.
///
/// The OUTPUT reading is scaled by `apply_chain_volume_db` because the
/// stream_tap reads the signal BEFORE the audio callback applies the
/// chain volume slider (#496). INPUT is untouched.
pub fn rebuild_stream_meters_row(
    engine_readings: &[StreamMeterReading],
    project_input_count: usize,
    chain_volume: f32,
    enabled: bool,
) -> Vec<crate::StreamMeter> {
    // #750: the per-stream graph is a LIVE surface — a disabled chain renders
    // no rows at all, overriding the `.max(1)` clamp below.
    if !enabled {
        return Vec::new();
    }
    let len = project_input_count.max(1);
    (0..len)
        .map(|i| match engine_readings.get(i) {
            Some(r) => crate::StreamMeter {
                in_dbfs: r.in_dbfs,
                out_dbfs: apply_chain_volume_db(r.out_dbfs, chain_volume),
            },
            None => crate::StreamMeter {
                in_dbfs: SILENT_DBFS,
                out_dbfs: SILENT_DBFS,
            },
        })
        .collect()
}

/// #85: how many meter rows the chain draws — one per STREAM, i.e. per
/// (input × output) pipeline, which is also how the engine indexes its
/// per-stream taps. A mid `Input`/`Output` is a stream of its own, so it gets
/// its own INPUT/OUTPUT bar; without this the row count came from the resolved
/// INPUTS and a mid port had no meter at all.
pub fn project_stream_count(
    chain: &project::chain::Chain,
    io_bindings: &[domain::io_binding::IoBinding],
) -> usize {
    engine::runtime_graph::chain_stream_count(chain, io_bindings)
}

/// Poll the per-stream subscriptions and return one
/// `StreamMeterReading` per stream for every chain in the store.
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
                    let (i, o) = compute_meter_for_chain(s.input.as_ref(), s.output.as_ref());
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

/// Meter polling interval in milliseconds. #715: this timer's per-frame work
/// (draining taps, rebuilding rows, the Slint re-render it triggers) is memory
/// traffic that competes with the audio worker on the shared cache and evicts
/// the NAM weights → cold-cache inference → late buffer → crackle. It must NOT
/// run faster than ~20 Hz (≥ 50 ms); 30 Hz (33 ms) is what caused the crackle.
pub(crate) const METER_POLL_TICK_MS: u64 = 66; // ~15 Hz

#[cfg(test)]
#[path = "meter_wiring_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "meter_wiring_signature_tests.rs"]
mod signature_tests;
