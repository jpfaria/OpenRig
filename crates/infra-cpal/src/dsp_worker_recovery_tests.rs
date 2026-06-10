//! Issue #670 — unit tests for the worker's saturation-recovery policy.
//! Owner-hit failure: once the ring pins at its overflow clamp (backlog 14)
//! the worker never recovers (kernel demotion keeps every buffer multi-ms).
//! The policy must demand recovery after a sustained run of saturated drains
//! and reset cleanly otherwise.

use super::dsp_worker::SaturationRecovery;

#[test]
fn no_recovery_below_threshold() {
    let mut r = SaturationRecovery::new(32);
    for _ in 0..31 {
        assert!(!r.observe(true), "must not trigger before the threshold");
    }
}

#[test]
fn recovery_after_sustained_saturation() {
    let mut r = SaturationRecovery::new(32);
    for _ in 0..31 {
        assert!(!r.observe(true));
    }
    assert!(
        r.observe(true),
        "32nd consecutive saturated drain must trigger"
    );
}

#[test]
fn unsaturated_drain_resets_the_run() {
    let mut r = SaturationRecovery::new(32);
    for _ in 0..20 {
        assert!(!r.observe(true));
    }
    assert!(!r.observe(false), "healthy drain resets");
    for _ in 0..31 {
        assert!(!r.observe(true), "run restarts from zero after a reset");
    }
    assert!(r.observe(true));
}

#[test]
fn retriggers_after_a_recovery() {
    let mut r = SaturationRecovery::new(4);
    for _ in 0..3 {
        assert!(!r.observe(true));
    }
    assert!(r.observe(true));
    // After a trigger the counter restarts — chronic saturation re-triggers.
    for _ in 0..3 {
        assert!(!r.observe(true));
    }
    assert!(r.observe(true));
}
