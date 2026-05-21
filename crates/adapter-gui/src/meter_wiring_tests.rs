use super::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use engine::output_meter::SILENT_DBFS;
use engine::spsc::SpscRing;

// ── refresh_subscriptions (issue: meters freeze after preset switch / toggle) ──

#[test]
fn refresh_subscriptions_resubscribes_when_chain_already_in_store() {
    // The fix's core property: even when a chain id is already in the
    // store, `refresh_subscriptions` must re-invoke the subscribe
    // closure so the latest runtime's rings replace the (possibly
    // dead) ones from before.
    let store = new_meter_store();
    let call_count: Rc<Cell<usize>> = Rc::new(Cell::new(0));
    let make_rings = {
        let call_count = call_count.clone();
        move |_: &domain::ids::ChainId| -> ChainMeterRings {
            call_count.set(call_count.get() + 1);
            ChainMeterRings {
                input: vec![Arc::new(SpscRing::<f32>::new(16, 0.0))],
                output: vec![Arc::new(SpscRing::<f32>::new(16, 0.0))],
            }
        }
    };
    let ids = vec![
        domain::ids::ChainId("rig:input-1".into()),
        domain::ids::ChainId("rig:input-2".into()),
    ];
    refresh_subscriptions(&store, &ids, &make_rings);
    assert_eq!(call_count.get(), 2);
    // Repeat — must re-subscribe (pre-fix: skipped, leaving dead rings).
    refresh_subscriptions(&store, &ids, &make_rings);
    assert_eq!(
        call_count.get(),
        4,
        "re-subscribe on every refresh so a runtime restart between \
         ticks doesn't leave the meter stuck at SILENT_DBFS"
    );
}

#[test]
fn refresh_subscriptions_drops_entries_for_chains_no_longer_present() {
    let store = new_meter_store();
    let make_rings = |_: &domain::ids::ChainId| ChainMeterRings {
        input: Vec::new(),
        output: Vec::new(),
    };
    refresh_subscriptions(
        &store,
        &[
            domain::ids::ChainId("rig:input-1".into()),
            domain::ids::ChainId("rig:input-2".into()),
        ],
        &make_rings,
    );
    assert_eq!(store.borrow().len(), 2);
    refresh_subscriptions(
        &store,
        &[domain::ids::ChainId("rig:input-1".into())],
        &make_rings,
    );
    assert_eq!(store.borrow().len(), 1);
    assert!(store
        .borrow()
        .contains_key(&domain::ids::ChainId("rig:input-1".into())));
}

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
