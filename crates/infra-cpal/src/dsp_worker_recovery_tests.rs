//! Issue #670 — unit tests for the worker's saturation-recovery policy.
//! Owner-hit failure: once the ring pins at its overflow clamp (backlog 14)
//! the worker never recovers (kernel demotion keeps every buffer multi-ms).
//! The policy must demand recovery after a sustained run of saturated drains
//! and reset cleanly otherwise.

use super::dsp_worker::{BudgetTracker, SaturationRecovery};

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

/// Issue #743 — a paused chain must NOT churn its RT computation budget.
/// The owner toggles the chain off/on: while paused the worker measures ~0
/// compute, which collapsed the window budget to the `period/10` floor; the
/// instant work resumes a buffer exceeds that floor and the fast-up re-declares
/// the policy back to 85 %. Each cycle = two `thread_policy_set` syscalls that
/// perturb the worker's scheduling and cause the 4-6 ms late buffers the owner
/// saw. An idle window must keep the standing budget, so resume costs no
/// re-declaration.
#[test]
fn pause_resume_cycles_do_not_churn_the_rt_budget() {
    const PERIOD_NS: u64 = 1_333_000; // 64 frames @ 48 kHz
    const WINDOW: usize = 2048; // BudgetTracker's window length
    let mut b = BudgetTracker::new(PERIOD_NS * 85 / 100);

    let mut redeclares = 0;
    for _cycle in 0..5 {
        // Paused: the draining chain processes ~nothing.
        for _ in 0..WINDOW {
            if b.observe(0, PERIOD_NS).is_some() {
                redeclares += 1;
            }
        }
        // Playing: a light ~11 %-of-period real cost (the owner's measured
        // compute was ~150 µs against a 1333 µs period).
        for _ in 0..WINDOW {
            if b.observe(150_000, PERIOD_NS).is_some() {
                redeclares += 1;
            }
        }
    }

    assert!(
        redeclares <= 2,
        "BUG #743: five pause/resume cycles re-declared the RT budget {redeclares} \
         times — an idle (paused) window collapsed the budget to the floor and \
         resume fast-up bounced it back to 85 %. Each re-declaration is a \
         thread_policy_set syscall that perturbs scheduling (the 4-6 ms late \
         buffers). A paused chain must keep its standing budget."
    );
}
