//! Issue #743 — toggling a chain ON must not run the full CoreAudio resolve.
//!
//! `chain_io_changed` (the toggle re-bind check) used `resolve_chain_audio_config`,
//! a CoreAudio device query costing hundreds of ms per device — ~750 ms on the
//! owner's four-device rig, on the GUI thread, every toggle-ON. But detecting a
//! re-bind only needs the BINDING topology (which device + channels each input/
//! output points at), which is already known cheaply from the binding registry
//! and the live stream signature. No CoreAudio needed.
//!
//! This pins the pure comparison: same device+channel topology ⇒ unchanged (the
//! common toggle-off→on resume path, no rebuild); a different device or channel
//! set ⇒ changed (a genuine re-bind ⇒ rebuild). Hardware-free.

use domain::ids::DeviceId;
use infra_cpal::io_topology_changed;

fn dev(id: &str, channels: &[usize]) -> (DeviceId, Vec<usize>) {
    (DeviceId(id.into()), channels.to_vec())
}

#[test]
fn identical_topology_is_unchanged() {
    let inputs = vec![dev("scarlett", &[0]), dev("teyun", &[0])];
    let outputs = vec![dev("out", &[0, 1])];
    assert!(
        !io_topology_changed(&inputs, &inputs, &outputs, &outputs),
        "the same device+channel topology must read as UNCHANGED — a toggle-on \
         resume must not trigger a rebuild (nor the CoreAudio resolve)"
    );
}

#[test]
fn a_rebound_input_device_is_changed() {
    let live = vec![dev("scarlett", &[0])];
    let bound = vec![dev("teyun", &[0])]; // user re-bound the input to another interface
    let outputs = vec![dev("out", &[0, 1])];
    assert!(
        io_topology_changed(&live, &bound, &outputs, &outputs),
        "a different input device must read as CHANGED (re-bind ⇒ rebuild)"
    );
}

#[test]
fn a_rebound_channel_is_changed() {
    let live = vec![dev("scarlett", &[0])];
    let bound = vec![dev("scarlett", &[1])]; // same device, different channel
    let outputs = vec![dev("out", &[0, 1])];
    assert!(
        io_topology_changed(&live, &bound, &outputs, &outputs),
        "the same device on a different channel must read as CHANGED"
    );
}

#[test]
fn a_rebound_output_is_changed() {
    let inputs = vec![dev("scarlett", &[0])];
    let live_out = vec![dev("out_a", &[0, 1])];
    let bound_out = vec![dev("out_b", &[0, 1])];
    assert!(
        io_topology_changed(&inputs, &inputs, &live_out, &bound_out),
        "a different output device must read as CHANGED"
    );
}
