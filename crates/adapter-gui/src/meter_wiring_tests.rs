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

// ── per-channel input subscription (multi-source InputBlock) ──

#[test]
fn split_rings_per_entry_returns_one_singleton_per_index() {
    let rings: Vec<Arc<SpscRing<f32>>> = (0..3).map(|_| ring_with(&[])).collect();
    let split = split_rings_per_entry(&rings);
    assert_eq!(split.len(), 3, "one slot per channel ring");
    for (i, slot) in split.iter().enumerate() {
        assert_eq!(slot.len(), 1, "each slot is a single ring (entry i={})", i);
    }
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
