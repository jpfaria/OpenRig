//! Issue #672 — the audio-thread processing seam.
//!
//! The CPAL input/output callbacks call these helpers, which read the chain's
//! *live* runtime through a [`LiveRuntimeSlot`] every buffer instead of holding
//! a fixed `Arc` captured at stream-build time. That single wait-free
//! `slot.load()` (an `Arc` refcount bump — no heap, no lock, no syscall) is the
//! only cost added to the audio thread, preserving invariant #8, and lets the
//! control worker swap a rebuilt runtime in without tearing the stream down.

use std::sync::Arc;

use engine::runtime::{process_input_f32, process_output_f32_mixed, ChainRuntimeState};

use crate::LiveRuntimeSlot;

/// Wrap each per-group runtime in a fresh [`LiveRuntimeSlot`] (issue #672).
///
/// The stream callbacks capture these slot handles and read them live, and the
/// controller stores the same slots so the control worker can publish a rebuilt
/// runtime into them without tearing the stream down.
#[must_use]
pub fn build_chain_slots(
    runtimes: &[(usize, Arc<ChainRuntimeState>)],
) -> Vec<(usize, LiveRuntimeSlot)> {
    runtimes
        .iter()
        .map(|(group, runtime)| (*group, LiveRuntimeSlot::new(Arc::clone(runtime))))
        .collect()
}

/// The slots a physical input device's stream must feed (issue #703):
/// every per-entry runtime whose cpal input index equals the stream's
/// device order. Two entries reading ONE device are two isolated runtimes
/// bound to the SAME stream — macOS Core Audio cannot open two streams on
/// one device, so the single callback fans out to all of them. Runtimes
/// without a per-entry identity (legacy whole-chain shape) fall back to
/// their group id, which historically WAS the cpal index.
// Only the CPAL stream-builders call this; the JACK-direct path (Linux+JACK)
// does not, so gate to its callers' cfg to stay warning-clean (#755).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[must_use]
pub(crate) fn slots_for_input_stream(
    slots: &[(usize, LiveRuntimeSlot)],
    cpal_index: usize,
) -> Vec<LiveRuntimeSlot> {
    slots
        .iter()
        .filter(|(group, slot)| slot.load().input_cpal_index().unwrap_or(*group) == cpal_index)
        .map(|(_, slot)| slot.handle())
        .collect()
}

/// The slots an output device's stream may mix. LAW (stream isolation): ONLY
/// the runtimes whose OWN binding outputs to THIS physical device — never "all
/// runtimes at the same rate".
///
/// Each per-input runtime writes its binding's output route and nothing else.
/// The old rate filter mixed every same-rate runtime into every same-rate
/// output, so a runtime that does NOT feed this device still got popped here —
/// its route is unwritten, the elastic buffer is empty, and every pop underruns
/// (N streams at one rate flood, invariant #4). `output_devices_by_input_cpal`
/// (from the resolved config) maps each runtime's input cpal index to its
/// binding's output device id(s); an empty map (JACK / degenerate) keeps the
/// pre-isolation behaviour of feeding the runtime.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[must_use]
pub(crate) fn slots_for_output_stream(
    slots: &[(usize, LiveRuntimeSlot)],
    output_devices_by_input_cpal: &[Vec<String>],
    output_device_id: &str,
) -> Vec<LiveRuntimeSlot> {
    slots
        .iter()
        .filter(|(group, slot)| {
            let cpal = slot.load().input_cpal_index().unwrap_or(*group);
            match output_devices_by_input_cpal.get(cpal) {
                Some(devs) => devs.iter().any(|d| d == output_device_id),
                None => true,
            }
        })
        .map(|(_, slot)| slot.handle())
        .collect()
}

/// Process one input buffer through the chain's live input runtime.
///
/// Wait-free: one `slot.load()` then the existing `process_input_f32`.
pub fn process_input_buffer(
    slot: &LiveRuntimeSlot,
    input_index: usize,
    data: &[f32],
    input_total_channels: usize,
) {
    process_input_f32(&slot.load(), input_index, data, input_total_channels);
}

/// Mix the chain's live per-group output runtimes into `out`.
///
/// `loaded` and `scratch` are caller-owned buffers captured once in the stream
/// callback (sized to the group count / output length), so this allocates
/// nothing per buffer: `loaded.clear()` + `push` reuses capacity and each
/// `slot.load()` only bumps an `Arc` refcount.
pub fn process_output_buffer(
    slots: &[LiveRuntimeSlot],
    loaded: &mut Vec<Arc<ChainRuntimeState>>,
    output_index: usize,
    out: &mut [f32],
    output_total_channels: usize,
    scratch: &mut [f32],
) {
    loaded.clear();
    for slot in slots {
        loaded.push(slot.load());
    }
    process_output_f32_mixed(loaded, output_index, out, output_total_channels, scratch);
}

#[cfg(test)]
mod issue_743_output_rate_isolation_tests {
    use super::*;
    use domain::ids::{ChainId, DeviceId};
    use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
    use engine::runtime::build_chain_runtime_state;
    use project::chain::Chain;

    fn pipe(rate: f32) -> LiveRuntimeSlot {
        let chain = Chain {
            id: ChainId("t".into()),
            description: None,
            instrument: "electric_guitar".into(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec!["io".into()],
            blocks: vec![],
            di_output: None,
        };
        let registry = vec![IoBinding {
            id: "io".into(),
            name: "IO".into(),
            inputs: vec![IoEndpoint {
                name: "in0".into(),
                device_id: DeviceId("d".into()),
                mode: ChannelMode::Mono,
                channels: vec![0],
            }],
            outputs: vec![IoEndpoint {
                name: "out0".into(),
                device_id: DeviceId("d".into()),
                mode: ChannelMode::Stereo,
                channels: vec![0, 1],
            }],
        }];
        let rt = build_chain_runtime_state(&chain, rate, &[256], &registry).unwrap();
        LiveRuntimeSlot::new(std::sync::Arc::new(rt))
    }

    #[test]
    fn an_output_device_mixes_only_the_runtimes_that_feed_it() {
        // FOUR isolated streams, ALL at 44.1 kHz, each on its OWN output
        // device. Per the isolation LAW, output device "outA" mixes ONLY the
        // runtime that outputs to "outA" — never the three same-rate siblings.
        // Mixing them would pop their unwritten routes = the underrun flood
        // ("4 streams at 44 are 4 separate pipelines, not one").
        let slots: Vec<(usize, LiveRuntimeSlot)> = vec![
            (0, pipe(44_100.0)),
            (1, pipe(44_100.0)),
            (2, pipe(44_100.0)),
            (3, pipe(44_100.0)),
        ];
        // input cpal index (group) -> its binding's output device id(s)
        let map = vec![
            vec!["outA".to_string()],
            vec!["outB".to_string()],
            vec!["outC".to_string()],
            vec!["outD".to_string()],
        ];
        for dev in ["outA", "outB", "outC", "outD"] {
            let mixed = slots_for_output_stream(&slots, &map, dev);
            assert_eq!(
                mixed.len(),
                1,
                "output '{dev}' must mix ONLY its own runtime, not the same-rate siblings"
            );
        }
    }

    #[test]
    fn runtimes_sharing_one_output_device_are_summed() {
        // Two inputs whose bindings both feed the SAME physical output device
        // (one interface's stereo out) ARE summed there — same device ⇒ same
        // rate, the legitimate backend sum, not cross-stream leakage.
        let slots: Vec<(usize, LiveRuntimeSlot)> = vec![(0, pipe(48_000.0)), (1, pipe(48_000.0))];
        let map = vec![vec!["shared".to_string()], vec!["shared".to_string()]];
        assert_eq!(slots_for_output_stream(&slots, &map, "shared").len(), 2);
    }
}
