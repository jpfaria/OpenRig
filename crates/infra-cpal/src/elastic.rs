//! Responsibility: sizes the elastic cushion of one output.
//! Per-output elastic-buffer target sizing for a chain.
//!
//! The engine's elastic buffer absorbs jitter between the input callback
//! and the output callback. The target depth depends on:
//!
//! 1. Backend — Linux+JACK uses a worker-thread DSP path with non-RT
//!    scheduling jitter, so it needs more headroom (8x buffer) than the
//!    direct CPAL callbacks on macOS/Windows (2x buffer).
//! 2. Endpoint kind — a regular output needs the default multiplier, but
//!    an Insert send already sees post-elastic samples and is followed by
//!    external hardware that has its own driver buffering, so doubling
//!    the headroom there is pure latency overhead.
//!
//! [`elastic_targets`] pairs each output stream with its right multiplier and
//! returns the per-output target depths in the same order; the cold build
//! (`compute_elastic_targets_for_chain`) and the live rebuild
//! (`elastic_targets_for_live_streams`) both go through it.

use domain::io_binding::IoBinding;
use engine::runtime::elastic_target_for_buffer;
use engine::runtime_endpoints::resolve_chain_io;
use project::chain::Chain;

use crate::resolved::ResolvedChainAudioConfig;

/// Backend-specific multiplier for the elastic buffer target.
/// JACK uses a worker-thread DSP path on Linux; non-RT scheduling jitter
/// needs more headroom than direct CPAL callbacks.
#[cfg(all(target_os = "linux", feature = "jack"))]
const ELASTIC_MULTIPLIER: u8 = 8;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
const ELASTIC_MULTIPLIER: u8 = 2;

/// Multiplier used for the elastic target of a regular output route.
/// See `ELASTIC_MULTIPLIER` for the per-backend rationale.
const ELASTIC_MULTIPLIER_REGULAR: u8 = ELASTIC_MULTIPLIER;
/// Multiplier used for the elastic target of an Insert block's *send*
/// endpoint. The main chain's elastic buffer already absorbs upstream
/// jitter before the signal reaches the insert send, and the external
/// hardware on the other side has its own driver buffering. Keeping the
/// send's elastic at the default multiplier would be pure redundancy
/// and roughly doubles the insert's round-trip latency; `1` trims that
/// overhead while the shared `ELASTIC_TARGET_FLOOR` prevents pathologic
/// sizing for tiny device buffers.
const ELASTIC_MULTIPLIER_INSERT_SEND: u8 = 1;

/// #965, macOS: multiplier for a regular output on the SAME device as one of
/// the chain's inputs. Every CoreAudio unit of a device runs on one HAL IO
/// thread, input first and output 0-5 us later, every cycle (measured on the
/// owner's Quantum HD 8 in both start orders, ~7000 cycles each). The DSP
/// runs on the #670 worker, so such an output needs one buffer for the
/// worker hand-off plus one of slack — exactly the insert send's cushion,
/// which runs live from the same worker pass. An output on another device
/// has another clock and another thread, and keeps the regular multiplier.
#[cfg(target_os = "macos")]
const ELASTIC_MULTIPLIER_SAME_DEVICE: u8 = 1;
#[cfg(not(target_os = "macos"))]
const ELASTIC_MULTIPLIER_SAME_DEVICE: u8 = ELASTIC_MULTIPLIER_REGULAR;

/// One stream endpoint as the sizing sees it: which device clock it runs on
/// and how many frames that device hands over per callback.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StreamClock<'a> {
    pub(crate) device_id: &'a str,
    pub(crate) buffer_frames: u32,
}

/// Per-output elastic targets for a chain, in the order of its output
/// streams: the regular outputs first, then one per bound Insert send.
///
/// #965: the ONE sizing for the cold build and the live rebuild. The live
/// path used to size every output at the regular multiplier — sends included
/// — so the first knob turn on an insert chain rebuilt the send (a gap in the
/// loop) and left it a buffer deeper than a fresh chain.
pub(crate) fn elastic_targets(
    chain: &Chain,
    registry: &[IoBinding],
    inputs: &[StreamClock<'_>],
    outputs: &[StreamClock<'_>],
) -> Vec<usize> {
    // The producer pushes a whole input callback at once: a route rests at
    // least one such burst deep, or its ring (2x the cushion) drops it.
    let burst = inputs
        .iter()
        .map(|i| i.buffer_frames as usize)
        .max()
        .unwrap_or(0);
    // Model A (#716): the regular (non-Insert) outputs come from the resolved
    // binding endpoints; Insert sends are appended after them in the output
    // streams, so the count still splits the two.
    let (_resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let regular_output_count: usize = resolved_outputs.len();
    outputs
        .iter()
        .enumerate()
        .map(|(idx, out)| {
            let multiplier = if idx >= regular_output_count {
                ELASTIC_MULTIPLIER_INSERT_SEND
            } else if inputs.iter().any(|i| i.device_id == out.device_id) {
                ELASTIC_MULTIPLIER_SAME_DEVICE
            } else {
                ELASTIC_MULTIPLIER_REGULAR
            };
            elastic_target_for_buffer(out.buffer_frames, multiplier).max(burst)
        })
        .collect()
}

/// [`elastic_targets`] for a chain being built from a fresh device resolve.
pub(crate) fn compute_elastic_targets_for_chain(
    chain: &Chain,
    resolved: &ResolvedChainAudioConfig,
    registry: &[IoBinding],
) -> Vec<usize> {
    let inputs: Vec<StreamClock<'_>> = resolved
        .stream_signature
        .inputs
        .iter()
        .map(|i| StreamClock {
            device_id: &i.device_id,
            buffer_frames: i.buffer_size_frames,
        })
        .collect();
    let outputs: Vec<StreamClock<'_>> = resolved
        .outputs
        .iter()
        .map(|o| StreamClock {
            device_id: &o.device_id,
            buffer_frames: crate::resolved_output_buffer_size_frames(o),
        })
        .collect();
    elastic_targets(chain, registry, &inputs, &outputs)
}

/// #740: [`elastic_targets`] for an I/O-UNCHANGED live rebuild, read straight
/// off the running streams' signature — no device resolve, so a
/// param/block/preset edit rebuilds the DSP off-thread instead of blocking
/// the GUI on CoreAudio.
#[cfg(not(all(target_os = "linux", feature = "jack")))] // CPAL live-rebuild path only (#755)
pub(crate) fn elastic_targets_for_live_streams(
    chain: &Chain,
    registry: &[IoBinding],
    signature: &crate::resolved::ChainStreamSignature,
) -> Vec<usize> {
    let inputs: Vec<StreamClock<'_>> = signature
        .inputs
        .iter()
        .map(|i| StreamClock {
            device_id: &i.device_id,
            buffer_frames: i.buffer_size_frames,
        })
        .collect();
    let outputs: Vec<StreamClock<'_>> = signature
        .outputs
        .iter()
        .map(|o| StreamClock {
            device_id: &o.device_id,
            buffer_frames: o.buffer_size_frames,
        })
        .collect();
    elastic_targets(chain, registry, &inputs, &outputs)
}

#[cfg(test)]
#[path = "elastic_tests.rs"]
mod tests;
