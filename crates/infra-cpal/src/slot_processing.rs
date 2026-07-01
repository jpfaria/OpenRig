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

/// The slots an output device's stream may mix (issue #743): only the runtimes
/// clocked at the output's own sample rate.
///
/// Each per-input runtime is isolated and clocked at ITS input device's rate
/// (#736). A runtime's output route is filled by that runtime's worker at its
/// own rate; an output stream pops it at the OUTPUT device's rate. When the two
/// rates differ (the owner's Scarlett @44.1 + TEYUN @48 rig) the mismatch is a
/// continuous under/overflow on the route — the output starves on almost every
/// pop (invariant #4: isolated streams must not be cross-rate mixed in our code;
/// the backend mixes same-rate streams). Mixing only same-rate runtimes keeps
/// each route's producer and consumer in lock-step. A single-output / single-rate
/// chain is unaffected (every runtime matches the one output rate).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
#[must_use]
pub(crate) fn slots_for_output_stream(
    slots: &[(usize, LiveRuntimeSlot)],
    output_sample_rate: f32,
) -> Vec<LiveRuntimeSlot> {
    slots
        .iter()
        .filter(|(_, slot)| (slot.load().sample_rate() - output_sample_rate).abs() < 1.0)
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
    fn an_output_stream_only_mixes_runtimes_at_its_own_rate() {
        // Two isolated interfaces at different rates (#736): Scarlett @44.1,
        // TEYUN @48. The 44.1 kHz output stream must consume ONLY the 44.1 kHz
        // runtimes — mixing a 48 kHz runtime's route into a 44.1 kHz output
        // (consumed slower than produced) is the owner's underrun flood (#743).
        let slots: Vec<(usize, LiveRuntimeSlot)> =
            vec![(0, pipe(44_100.0)), (1, pipe(44_100.0)), (2, pipe(48_000.0)), (3, pipe(48_000.0))];

        let at_44k = slots_for_output_stream(&slots, 44_100.0);
        assert_eq!(
            at_44k.len(),
            2,
            "the 44.1 kHz output must mix exactly the two 44.1 kHz runtimes, not the 48 kHz ones"
        );
        assert!(
            at_44k.iter().all(|s| (s.load().sample_rate() - 44_100.0).abs() < 1.0),
            "every mixed runtime must be at the output's own rate"
        );

        let at_48k = slots_for_output_stream(&slots, 48_000.0);
        assert_eq!(at_48k.len(), 2, "the 48 kHz output must mix exactly the two 48 kHz runtimes");
    }
}
