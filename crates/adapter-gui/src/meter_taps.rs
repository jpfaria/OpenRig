//! Responsibility: keeps a tap subscribed for every stream a chain has.

use crate::meter_math::compute_meter_for_chain;

use application::audio_taps::{AudioTap, AudioTaps, TapPoint};
use std::sync::Arc;

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

pub(crate) const METER_POLL_TICK_MS: u64 = 66; // ~15 Hz
