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
