//! Issue #743 — cheap re-bind detection for the chain toggle path.

use domain::ids::DeviceId;

/// `true` when the device+channel topology of the live streams differs from the
/// binding-resolved topology — i.e. the user re-bound the chain's E/S.
///
/// Pure: no CoreAudio query. The toggle-ON re-bind check (`chain_io_changed`)
/// used the full `resolve_chain_audio_config`, a device query costing hundreds
/// of ms per device (~750 ms on a four-device rig) on the GUI thread every time
/// a chain is enabled — but detecting a re-bind only needs the device + channel
/// identity, which the binding registry and the live stream signature already
/// carry. A rate/buffer change reaches the runtime through the device-settings
/// sync, not this per-chain toggle path, so the cheap comparison is sufficient
/// here.
///
/// Order-sensitive: both sides come from `resolve_chain_io`, whose ordering is
/// deterministic (binding-registry order), so equal bindings compare equal.
pub fn io_topology_changed(
    live_inputs: &[(DeviceId, Vec<usize>)],
    bound_inputs: &[(DeviceId, Vec<usize>)],
    live_outputs: &[(DeviceId, Vec<usize>)],
    bound_outputs: &[(DeviceId, Vec<usize>)],
) -> bool {
    live_inputs != bound_inputs || live_outputs != bound_outputs
}
