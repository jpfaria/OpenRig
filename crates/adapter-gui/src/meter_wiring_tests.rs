use super::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use engine::output_meter::SILENT_DBFS;
use engine::spsc::SpscRing;

fn ring_with(samples: &[f32]) -> Arc<SpscRing<f32>> {
    let r = Arc::new(SpscRing::<f32>::new(16, 0.0));
    for &s in samples {
        r.push(s);
    }
    r
}

#[test]
fn empty_taps_return_silent_silent() {
    let (i, o) = compute_meter_for_chain(&[], &[]);
    assert_eq!(i, SILENT_DBFS);
    assert_eq!(o, SILENT_DBFS);
}

#[test]
fn input_only_signal_returns_input_db_and_silent_out() {
    let input = vec![ring_with(&[0.5])];
    let (i, o) = compute_meter_for_chain(&input, &[]);
    assert!((i - (-6.02)).abs() < 0.05, "in={i}");
    assert_eq!(o, SILENT_DBFS);
}

#[test]
fn output_only_signal_returns_silent_in_and_output_db() {
    let output: Vec<Arc<SpscRing<f32>>> = vec![ring_with(&[0.25]), ring_with(&[])];
    let (i, o) = compute_meter_for_chain(&[], &output);
    assert_eq!(i, SILENT_DBFS);
    assert!((o - (-12.04)).abs() < 0.05, "out={o}");
}

#[test]
fn both_taps_report_independent_peaks() {
    let input = vec![ring_with(&[0.5])];
    let output = vec![ring_with(&[0.9]), ring_with(&[])];
    let (i, o) = compute_meter_for_chain(&input, &output);
    let want_in = 20.0_f32 * 0.5_f32.log10();
    let want_out = 20.0_f32 * 0.9_f32.log10();
    assert!((i - want_in).abs() < 0.05);
    assert!((o - want_out).abs() < 0.05);
}

#[test]
fn above_full_scale_reports_positive_for_clip_indicator() {
    let output = vec![ring_with(&[1.5]), ring_with(&[])];
    let (_, o) = compute_meter_for_chain(&[], &output);
    assert!(o > 0.0, "above 1.0 should be > 0 dBFS, got {o}");
}

// ── apply_chain_volume_db (issue #496: OUTPUT meter must respond
// ── to the chain volume slider). Chain volume is applied in the
// ── audio callback AFTER the stream_tap, so the GUI has to scale
// ── the OUTPUT reading by `20·log10(volume_pct/100)` to reflect
// ── what actually reaches the DAC.

#[test]
fn apply_chain_volume_at_unity_is_identity() {
    assert!((apply_chain_volume_db(-12.0, 100.0) - (-12.0)).abs() < 1e-3);
}

#[test]
fn apply_chain_volume_at_200pct_adds_6_db() {
    let v = apply_chain_volume_db(-12.0, 200.0);
    assert!((v - (-6.0)).abs() < 0.1, "got {v}");
}

#[test]
fn apply_chain_volume_at_50pct_subtracts_6_db() {
    let v = apply_chain_volume_db(-6.0, 50.0);
    assert!((v - (-12.0)).abs() < 0.1, "got {v}");
}

#[test]
fn apply_chain_volume_at_zero_is_silent() {
    assert_eq!(apply_chain_volume_db(-12.0, 0.0), SILENT_DBFS);
}

#[test]
fn apply_chain_volume_preserves_silent_reading() {
    assert_eq!(apply_chain_volume_db(SILENT_DBFS, 200.0), SILENT_DBFS);
    assert_eq!(apply_chain_volume_db(SILENT_DBFS, 50.0), SILENT_DBFS);
}

#[test]
fn apply_chain_volume_at_125pct_adds_about_1_94_db() {
    let v = apply_chain_volume_db(-20.0, 125.0);
    assert!((v - (-18.06)).abs() < 0.1, "got {v}");
}

// ── per-stream meters (user ask: multi-input chains, 21 May 2026) ──

#[test]
fn poll_per_stream_returns_one_reading_per_stream() {
    let store = new_meter_store_per_stream();
    let id = domain::ids::ChainId("rig:input-1".into());
    let make_streams = |_: &domain::ids::ChainId| ChainMeterStreams {
        streams: vec![
            StreamMeterRings {
                input: vec![ring_with(&[0.5])],
                output: vec![ring_with(&[0.9]), ring_with(&[])],
            },
            StreamMeterRings {
                input: vec![ring_with(&[0.1])],
                output: vec![ring_with(&[0.25]), ring_with(&[])],
            },
        ],
    };
    refresh_subscriptions_lazy_per_stream(&store, &[id.clone()], &[], &make_streams);
    let readings = poll_per_stream(&store);
    assert_eq!(readings.len(), 1, "one chain");
    let chain_readings = &readings[0];
    assert_eq!(chain_readings.0, id);
    assert_eq!(chain_readings.1.len(), 2, "two streams");
    let s0 = &chain_readings.1[0];
    let s1 = &chain_readings.1[1];
    let want_s0_in = 20.0_f32 * 0.5_f32.log10();
    let want_s0_out = 20.0_f32 * 0.9_f32.log10();
    let want_s1_in = 20.0_f32 * 0.1_f32.log10();
    let want_s1_out = 20.0_f32 * 0.25_f32.log10();
    assert!((s0.in_dbfs - want_s0_in).abs() < 0.05);
    assert!((s0.out_dbfs - want_s0_out).abs() < 0.05);
    assert!((s1.in_dbfs - want_s1_in).abs() < 0.05);
    assert!((s1.out_dbfs - want_s1_out).abs() < 0.05);
}

// ── lazy + invalidate (fix flicker: never re-subscribe mid-stream) ──

#[test]
fn refresh_subscriptions_lazy_per_stream_skips_when_entry_already_present() {
    let store = new_meter_store_per_stream();
    let id = domain::ids::ChainId("rig:input-1".into());
    let calls: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let make_streams = {
        let calls = calls.clone();
        move |_: &domain::ids::ChainId| {
            calls.set(calls.get() + 1);
            ChainMeterStreams {
                streams: vec![StreamMeterRings {
                    input: vec![Arc::new(SpscRing::<f32>::new(16, 0.0))],
                    output: vec![Arc::new(SpscRing::<f32>::new(16, 0.0))],
                }],
            }
        }
    };

    refresh_subscriptions_lazy_per_stream(&store, &[id.clone()], &[], &make_streams);
    assert_eq!(calls.get(), 1, "first call subscribes");

    // Repeat with no invalidation: must skip — stale rings would
    // otherwise flicker the meter every tick (~30 Hz).
    refresh_subscriptions_lazy_per_stream(&store, &[id.clone()], &[], &make_streams);
    assert_eq!(
        calls.get(),
        1,
        "no invalidation ⇒ no re-subscribe (no flicker)"
    );

    // Caller invalidates explicitly (e.g. on chain toggle / rig-nav).
    refresh_subscriptions_lazy_per_stream(
        &store,
        &[id.clone()],
        &[id.clone()],
        &make_streams,
    );
    assert_eq!(
        calls.get(),
        2,
        "invalidated chain re-subscribes on the next tick"
    );
}

#[test]
fn refresh_subscriptions_lazy_per_stream_drops_missing_chains() {
    let store = new_meter_store_per_stream();
    let make_streams = |_: &domain::ids::ChainId| ChainMeterStreams {
        streams: vec![StreamMeterRings::default()],
    };
    refresh_subscriptions_lazy_per_stream(
        &store,
        &[
            domain::ids::ChainId("rig:input-1".into()),
            domain::ids::ChainId("rig:input-2".into()),
        ],
        &[],
        &make_streams,
    );
    assert_eq!(store.borrow().len(), 2);
    refresh_subscriptions_lazy_per_stream(
        &store,
        &[domain::ids::ChainId("rig:input-1".into())],
        &[],
        &make_streams,
    );
    assert_eq!(store.borrow().len(), 1);
}

// ── chain signature tracking (re-subscribe on ANY change) ──

#[test]
fn chain_signature_changes_when_enabled_flag_flips() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let mut c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
    };
    let s1 = chain_meter_signature(&c);
    c.enabled = true;
    let s2 = chain_meter_signature(&c);
    assert_ne!(s1, s2, "enabled flip must change the signature");
}

#[test]
fn chain_signature_changes_when_block_enabled_bit_flips() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let mut c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
    };
    let s1 = chain_meter_signature(&c);
    c.blocks[0].enabled = false;
    let s2 = chain_meter_signature(&c);
    assert_ne!(s1, s2, "per-block enabled flip (scene bypass) must \
         change the signature so the meter re-subscribes after scene \
         switch even when block ids don't change");
}

#[test]
fn chain_signature_stable_when_only_param_value_changes() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    use domain::value_objects::ParameterValue;
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(0.5));
    let mut c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params,
            }),
        }],
    };
    let s1 = chain_meter_signature(&c);
    // Just a knob movement — doesn't restart the runtime, must NOT
    // invalidate the meter (would cause the flicker that #flicker-fix
    // killed).
    if let AudioBlockKind::Core(core) = &mut c.blocks[0].kind {
        core.params.insert("gain", ParameterValue::Float(0.7));
    }
    let s2 = chain_meter_signature(&c);
    assert_eq!(s1, s2, "param value change without runtime restart \
         must keep the signature stable");
}

// ── timer-shape signature: tracks engine runtime layout, not just
//     project-side bits. User-reported bug: toggle off → on, meters
//     never recover. Project state alone (`chain.enabled`) flips
//     correctly, but the timer's invalidation must ALSO catch the
//     engine tearing down + rebuilding the per-input runtimes —
//     otherwise the cached rings point at producers that no longer
//     push (the old runtime instance got dropped by remove_chain).

#[test]
fn timer_signature_flips_on_toggle_off_then_on_cycle() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let mut c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
    };
    // Initial: enabled, runtime up with 3 streams.
    let s_initial = crate::meter_wiring::timer_chain_signature(&c, 3);
    // Toggle off: enabled flips to false, engine tears down → stream_count=0.
    c.enabled = false;
    let s_off = crate::meter_wiring::timer_chain_signature(&c, 0);
    assert_ne!(s_initial, s_off,
        "toggle off must flip the signature so the timer invalidates \
         the dead ring handles left by the torn-down runtime");
    // Toggle on: enabled flips back to true, fresh runtime → stream_count=3.
    c.enabled = true;
    let s_on = crate::meter_wiring::timer_chain_signature(&c, 3);
    assert_ne!(s_off, s_on,
        "toggle on must flip the signature again so the timer drops \
         the empty cache entry and re-subscribes against the freshly \
         rebuilt per-input runtimes");
}

#[test]
fn controller_offline_then_back_invalidates_every_chain() {
    // User-reported: toggle off the last enabled chain → meter dies
    // and never recovers on toggle on. Root cause: when the whole
    // chain set goes empty, `sync_live_chain_runtime` drops the
    // `ProjectRuntimeController`. The timer's early return on the
    // None controller leaves the per-chain signature cache intact,
    // so when the next toggle-on call instantiates a FRESH
    // controller, the same project state yields the same cached
    // signature → no invalidation fires → the store keeps handing
    // out rings allocated against the dropped controller (nothing
    // pushes to them). The fix is to wipe the cache whenever the
    // controller is offline so the next online tick treats every
    // chain as a fresh subscription.
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let chain = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
    };
    let mut last_sig: std::collections::HashMap<domain::ids::ChainId, u64> =
        std::collections::HashMap::new();
    let api_on = RecordingTapApi { stream_count: 3, ..Default::default() };
    // Tick 1: controller online with 3-stream chain.
    let inv1 = crate::meter_wiring::detect_invalidations(
        std::slice::from_ref(&chain),
        Some(&api_on),
        &mut last_sig,
    );
    assert_eq!(inv1.len(), 1, "first online tick subscribes the chain");
    assert!(last_sig.contains_key(&chain.id), "signature cached");
    // Tick 2: controller offline (last chain toggled off).
    let _ = crate::meter_wiring::detect_invalidations::<RecordingTapApi>(
        std::slice::from_ref(&chain),
        None,
        &mut last_sig,
    );
    assert!(
        last_sig.is_empty(),
        "offline tick must wipe the cache; otherwise a fresh \
         controller with identical project state would compare \
         equal against the stale entry and skip the re-subscribe"
    );
    // Tick 3: fresh controller comes back (different api instance,
    // same project state).
    let api_on2 = RecordingTapApi { stream_count: 3, ..Default::default() };
    let inv3 = crate::meter_wiring::detect_invalidations(
        std::slice::from_ref(&chain),
        Some(&api_on2),
        &mut last_sig,
    );
    assert_eq!(
        inv3.len(),
        1,
        "fresh controller must invalidate every chain so dead rings \
         get re-subscribed against the new per-input runtimes"
    );
}

#[test]
fn timer_signature_stays_constant_across_steady_state_ticks() {
    use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
    use project::chain::Chain;
    use project::param::ParameterSet;
    let c = Chain {
        id: domain::ids::ChainId("c1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        blocks: vec![AudioBlock {
            id: domain::ids::BlockId("b1".into()),
            enabled: true,
            kind: AudioBlockKind::Core(CoreBlock {
                effect_type: "gain".into(),
                model: "volume".into(),
                params: ParameterSet::default(),
            }),
        }],
    };
    // Two consecutive steady-state ticks: same enabled, same blocks,
    // same stream_count → signature stable, no re-subscribe (flicker
    // regression guard).
    let a = crate::meter_wiring::timer_chain_signature(&c, 3);
    let b = crate::meter_wiring::timer_chain_signature(&c, 3);
    assert_eq!(a, b, "steady-state ticks must NOT re-subscribe");
}

// ── build_streams_from_taps: per-entry meter routing.
// Original bug (3-source screenshot): input shows in slot 3, output
// only in slot 1. Pinned causes covered here historically:
//   (a) input was subscribed as channels [0..N] of runtime[0] instead
//       of one ring per per-input runtime (issue #350 layout);
//   (b) the chain stream_tap was subscribed ONCE (stream_index=0) and
//       its Arc cloned into every row — SPSC ring has a single
//       consumer, so row 0 drains it and rows 1..N see empty.
// Issue #557 closed the follow-up "stream 1 silent + wrong device
// channel" gap: the helper now drives a single high-level call —
// `subscribe_stream_input_tap(cid, i, cap)` — and the controller
// resolves runtime, cpal group, and endpoint channel inside.
// The new helper must:
//   - call subscribe_stream_input_tap once per global stream_index 0..N
//   - call subscribe_stream_tap once per global stream_index 0..N
//   - place each subscription in its own row (no Arc broadcasting).

#[derive(Default)]
struct RecordingTapApi {
    stream_count: usize,
    stream_input_calls: std::cell::RefCell<Vec<usize>>, // stream_index
    stream_calls: std::cell::RefCell<Vec<usize>>,       // stream_index
}

impl crate::meter_wiring::MeterTapApi for RecordingTapApi {
    fn stream_count(&self, _cid: &domain::ids::ChainId) -> usize {
        self.stream_count
    }
    fn subscribe_stream_input_tap(
        &self,
        _cid: &domain::ids::ChainId,
        stream_index: usize,
        _capacity: usize,
    ) -> Option<Arc<SpscRing<f32>>> {
        self.stream_input_calls.borrow_mut().push(stream_index);
        Some(Arc::new(SpscRing::<f32>::new(16, 0.0)))
    }
    fn subscribe_stream_tap(
        &self,
        _cid: &domain::ids::ChainId,
        stream_index: usize,
        _capacity: usize,
    ) -> Option<[Arc<SpscRing<f32>>; 2]> {
        self.stream_calls.borrow_mut().push(stream_index);
        Some([
            Arc::new(SpscRing::<f32>::new(16, 0.0)),
            Arc::new(SpscRing::<f32>::new(16, 0.0)),
        ])
    }
}

#[test]
fn build_streams_subscribes_stream_input_tap_once_per_global_index() {
    let api = RecordingTapApi { stream_count: 3, ..Default::default() };
    let cid = domain::ids::ChainId("c1".into());
    let _ = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert_eq!(
        *api.stream_input_calls.borrow(),
        vec![0_usize, 1, 2],
        "input meter must subscribe via the per-stream API once per \
         global stream index; the controller resolves the runtime, \
         cpal group, and endpoint channel — issue #557"
    );
}

#[test]
fn build_streams_subscribes_one_stream_tap_per_global_index_not_just_zero() {
    let api = RecordingTapApi { stream_count: 3, ..Default::default() };
    let cid = domain::ids::ChainId("c1".into());
    let _ = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert_eq!(
        *api.stream_calls.borrow(),
        vec![0_usize, 1, 2],
        "stream_tap must be subscribed once per GLOBAL stream index so \
         each row gets its own ring; sharing one ring across rows means \
         the first row drains the SPSC and rows 1..N stay silent"
    );
}

#[test]
fn build_streams_produces_one_row_per_stream_with_distinct_rings() {
    let api = RecordingTapApi { stream_count: 3, ..Default::default() };
    let cid = domain::ids::ChainId("c1".into());
    let chain = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert_eq!(chain.streams.len(), 3, "one row per stream");
    // Every row must hold rings allocated by its OWN subscribe call —
    // not Arc::clone of the previous row's rings.
    let out_ptrs: Vec<*const SpscRing<f32>> = chain
        .streams
        .iter()
        .flat_map(|s| s.output.iter().map(|r| Arc::as_ptr(r)))
        .collect();
    let unique: std::collections::HashSet<_> = out_ptrs.iter().collect();
    assert_eq!(
        unique.len(),
        out_ptrs.len(),
        "output rings must be distinct across rows; sharing was the \
         bug — first row drained the SPSC, others saw silence"
    );
}

#[test]
fn build_streams_returns_empty_when_chain_has_no_runtime() {
    let api = RecordingTapApi { stream_count: 0, ..Default::default() };
    let cid = domain::ids::ChainId("c1".into());
    let chain = crate::meter_wiring::build_streams_from_taps(&api, &cid, 4096);
    assert!(
        chain.streams.is_empty(),
        "no runtime ⇒ no meter rows; UI shows the empty list and the \
         row count converges back the next tick when the runtime is \
         rebuilt"
    );
}
