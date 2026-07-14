//! Issue #760 — the OS-workgroup join must target the device the callback
//! actually serves, not the system default.
//!
//! Bug: `join()` resolves `kAudioHardwarePropertyDefaultInputDevice` /
//! `…DefaultOutputDevice` unconditionally, so in a **multi-device** rig every
//! callback thread joins the *default* device's workgroup. The thread serving
//! the non-default interface (e.g. a 48 kHz TEYUN while a 44.1 kHz Scarlett is
//! the system default) is then NOT co-scheduled with its own device's IO
//! thread → under CPU contention it is preempted → steady-state underruns on
//! that chain, with CPU to spare. Single-device tests never caught it because
//! there the bound device *is* the default.
//!
//! This pins the pure decision layer: given the device a stream is bound to,
//! which device's workgroup should its callback join? It must be the bound
//! device — never a hard-coded system default.

use super::{workgroup_join_target, WorkgroupTarget};

#[test]
fn join_target_follows_the_bound_device_not_the_system_default() {
    // A callback bound to a specific (non-default) device UID must target THAT
    // device's workgroup. Today the decision ignores the arg and collapses to
    // the system default → RED.
    let bound = "coreaudio:TEYUN-Q26-48k";
    assert_eq!(
        workgroup_join_target(Some(bound)),
        WorkgroupTarget::Device(bound.to_string()),
        "a callback bound to a non-default device must join THAT device's \
         workgroup, not the system default"
    );
}

#[test]
fn join_target_falls_back_to_system_default_only_when_no_device_is_bound() {
    // Legacy/single-device path (no bound device id available) may still fall
    // back to the system default — that is the only sanctioned use of it.
    assert_eq!(
        workgroup_join_target(None),
        WorkgroupTarget::SystemDefault,
        "with no bound device, falling back to the system default is allowed"
    );
}

/// Issue #779 — a thread that joined the device OS workgroup MUST leave it
/// before it exits. The join was previously leaked with no matching
/// `os_workgroup_leave`, so when the dsp-worker thread is torn down on a chain
/// rebuild libpthread runs `_os_workgroup_tsd_cleanup` on the dying thread and
/// crashes (EXC_BREAKPOINT). The `WorkgroupMembership` guard leaves on drop.
#[cfg(target_os = "macos")]
mod membership_lifecycle {
    use super::super::imp::{JoinToken, WorkgroupMembership, LEAVE_COUNT};
    use std::sync::atomic::Ordering;

    #[test]
    fn a_successful_join_owes_a_leave_a_failed_one_does_not() {
        // rc == 0 → this thread became a member and must leave before exiting.
        let joined =
            WorkgroupMembership::from_join(0, std::ptr::null_mut(), Box::new(JoinToken::zeroed()));
        assert!(
            joined.owes_leave(),
            "a successful os_workgroup_join must be balanced by a leave (#779)"
        );
        // Forget rather than drop: this test asserts the DECISION only, and must
        // not touch the process-wide LEAVE_COUNT that the guard test observes.
        std::mem::forget(joined);
        // rc != 0 (failure, or already-a-member) → no membership taken.
        let failed =
            WorkgroupMembership::from_join(-1, std::ptr::null_mut(), Box::new(JoinToken::zeroed()));
        assert!(!failed.owes_leave());
    }

    // The two counter cases live in ONE test so the process-wide LEAVE_COUNT is
    // observed sequentially (cargo runs tests concurrently); the decision test
    // above forgets its active guard so nothing else mutates the counter.
    #[test]
    fn a_joined_thread_leaves_on_exit_and_an_inactive_one_does_not() {
        // A worker thread joins, then exits — the guard must leave on the way out.
        let before = LEAVE_COUNT.load(Ordering::Relaxed);
        std::thread::spawn(|| {
            let _membership = WorkgroupMembership::from_join(
                0,
                std::ptr::null_mut(),
                Box::new(JoinToken::zeroed()),
            );
        })
        .join()
        .unwrap();
        assert_eq!(
            LEAVE_COUNT.load(Ordering::Relaxed) - before,
            1,
            "a dsp-worker that joined the OS workgroup must os_workgroup_leave before its \
             thread exits — otherwise _os_workgroup_tsd_cleanup crashes (#779)"
        );

        // A failed join owes no leave and must not touch the counter.
        let after_join = LEAVE_COUNT.load(Ordering::Relaxed);
        drop(WorkgroupMembership::from_join(
            -1,
            std::ptr::null_mut(),
            Box::new(JoinToken::zeroed()),
        ));
        assert_eq!(
            LEAVE_COUNT.load(Ordering::Relaxed),
            after_join,
            "a failed join owes no leave and must not touch the counter"
        );
    }

    /// Real-hardware reproduction of #779 (`OPENRIG_HW_TESTS=1`, macOS). A
    /// spawned thread joins the REAL default-input OS workgroup (as the
    /// dsp-worker does) and then EXITS. Before the fix this crashed the process
    /// in libpthread's `_os_workgroup_tsd_cleanup` (EXC_BREAKPOINT); with the
    /// guard the thread leaves the workgroup on exit. Reaching the end means the
    /// process survived the thread teardown. Gated: headless CI has no device.
    #[test]
    fn worker_thread_join_then_exit_does_not_crash_on_real_hardware() {
        if std::env::var_os("OPENRIG_HW_TESTS").is_none() {
            eprintln!(
                "[#779 HW] SKIPPED — real-hardware workgroup test. Run with OPENRIG_HW_TESTS=1."
            );
            return;
        }
        let before = LEAVE_COUNT.load(Ordering::Relaxed);
        std::thread::spawn(|| {
            let _membership = super::super::join_input(None);
        })
        .join()
        .expect("worker thread must exit cleanly, not crash on workgroup TSD cleanup (#779)");
        let leaves = LEAVE_COUNT.load(Ordering::Relaxed) - before;
        eprintln!("[#779 HW] worker thread joined the real workgroup and exited cleanly (leaves={leaves})");
    }
}
