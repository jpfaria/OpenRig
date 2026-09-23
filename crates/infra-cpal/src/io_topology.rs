//! Responsibility: computes the topology signatures a rebuild decision compares.
//!
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

/// The device+channel signature the chain's streams MUST have, insert loops
/// included (#881).
///
/// `resolve_chain_io` answers only what the chain's own bindings point at —
/// head/tail plus mid ports. A bound `Insert` adds two more streams:
/// its SEND is an output and its RETURN is an input, exactly the shims
/// `effective_inputs` / `effective_outputs` append when the graph is built. A
/// comparison blind to them reported "I/O unchanged" when the user added or
/// bound an insert on a RUNNING chain, so the live-edit path swapped only the
/// DSP and kept the old streams: the post-insert segment then waited on a
/// return stream nobody had opened and the rig went silent until a restart.
/// Its only caller, `chain_io_changed`, is cpal-only — the JACK twin returns
/// `Ok(false)` while the stream-topology live-swap stays unwired (#672).
#[cfg_attr(all(target_os = "linux", feature = "jack"), allow(dead_code))]
pub(crate) fn bound_io_signature(
    chain: &project::chain::Chain,
    registry: &[domain::io_binding::IoBinding],
) -> (Vec<(DeviceId, Vec<usize>)>, Vec<(DeviceId, Vec<usize>)>) {
    let (bound_in, bound_out) = engine::runtime_endpoints::resolve_chain_io(chain, registry);
    let mut inputs: Vec<(DeviceId, Vec<usize>)> = bound_in
        .into_iter()
        .map(|e| (e.device_id, e.channels))
        .collect();
    let mut outputs: Vec<(DeviceId, Vec<usize>)> = bound_out
        .into_iter()
        .map(|e| (e.device_id, e.channels))
        .collect();

    // #967: a BOUND insert owns its send and return whether the block is
    // enabled or not (`engine::insert_cut::insert_owns_streams`). Counting only
    // enabled ones made a footswitch press read as a re-bind, so a one-bit flip
    // closed and reopened every stream the chain owned (2–3 s of silence on the
    // owner's rig). Switching an insert changes only the DSP cut.
    for block in chain
        .blocks
        .iter()
        .filter(|b| engine::insert_cut::insert_owns_streams(b, registry))
    {
        let project::block::AudioBlockKind::Insert(insert) = &block.kind else {
            continue;
        };
        // Both sides or nothing — `insert_owns_streams` already required it.
        let (Some(ret), Some(send)) = (
            crate::chain_resolve::insert_return_as_input_entry(insert, registry),
            crate::chain_resolve::insert_send_as_output_entry(insert, registry),
        ) else {
            continue;
        };
        inputs.push((ret.device_id, ret.channels));
        outputs.push((send.device_id, send.channels));
    }
    (inputs, outputs)
}

#[cfg(test)]
#[path = "io_topology_tests.rs"]
mod io_topology_tests;

/// The chain's STRUCTURE as the streams see it (#881): one entry per block —
/// its id and model identity, plus the enabled flag for the ports (`Input` /
/// `Output`), whose on/off state changes the streams the chain owns — and the
/// runtime grouping those streams are bound to (#967). Parameter values are
/// deliberately absent: a knob turn is a DSP edit, not a new topology.
pub(crate) fn chain_structure_signature(
    chain: &project::chain::Chain,
    registry: &[domain::io_binding::IoBinding],
) -> Vec<String> {
    chain
        .blocks
        .iter()
        .map(|b| {
            // #967: an INSERT is routing, but its enable flag does not change
            // the streams it owns (`engine::insert_cut`) — only the DSP cut,
            // which a live rebuild carries. Leaving the flag in here sent every
            // footswitch press through a full stream rebuild (2–3 s of
            // silence, measured on the owner's rig).
            if matches!(b.kind, project::block::AudioBlockKind::Insert(_)) {
                format!("{}|{}", b.id.0, b.kind.model_identity())
            } else if b.kind.is_routing() {
                format!("{}|{}|{}", b.id.0, b.kind.model_identity(), b.enabled)
            } else {
                format!("{}|{}", b.id.0, b.kind.model_identity())
            }
        })
        .chain(std::iter::once(format!(
            // #967: the runtimes the chain is split into are bound to its
            // streams; a change that regroups them (an insert switched on a
            // multi-E/S chain) needs new streams, one that does not (a
            // single-E/S chain) stays a DSP rebuild.
            "groups|{:?}",
            engine::runtime_graph::input_group_ids(chain, registry)
        )))
        .collect()
}
