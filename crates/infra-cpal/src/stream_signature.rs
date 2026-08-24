//! Responsibility: derives a chain's stream signature from its resolved IO.
//!
//! The signature is what `chain_resolve` compares to decide whether a
//! live chain's streams can stay up or have to be rebuilt.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use domain::io_binding::IoBinding;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use engine::runtime_endpoints::{resolve_chain_io, InputEntry, OutputEntry};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::chain::Chain;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::{
    ChainStreamSignature, InputStreamSignature, OutputStreamSignature, ResolvedInputDevice,
    ResolvedOutputDevice,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::stream_config::{
    resolved_input_buffer_size_frames, resolved_input_sample_rate,
    resolved_output_buffer_size_frames, resolved_output_sample_rate,
};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_chain_stream_signature_multi(
    chain: &Chain,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
    registry: &[IoBinding],
) -> ChainStreamSignature {
    // Model A (#716): the chain's input/output endpoints come from the binding
    // registry, not from block `entries`. The resolved order matches the
    // `inputs`/`outputs` device vectors (both built from `resolve_chain_io`).
    let (resolved_inputs, resolved_outputs) = resolve_chain_io(chain, registry);
    let chain_input_entries: Vec<&InputEntry> = resolved_inputs.iter().collect();
    let input_sigs: Vec<InputStreamSignature> = if !chain_input_entries.is_empty() {
        chain_input_entries
            .iter()
            .zip(inputs.iter())
            .map(|(ci, ri)| InputStreamSignature {
                device_id: ci.device_id.0.clone(),
                channels: ci.channels.clone(),
                stream_channels: ri.supported.channels(),
                sample_rate: resolved_input_sample_rate(ri),
                buffer_size_frames: resolved_input_buffer_size_frames(ri),
            })
            .collect()
    } else {
        inputs
            .iter()
            .map(|ri| InputStreamSignature {
                device_id: String::new(),
                channels: Vec::new(),
                stream_channels: ri.supported.channels(),
                sample_rate: resolved_input_sample_rate(ri),
                buffer_size_frames: resolved_input_buffer_size_frames(ri),
            })
            .collect()
    };

    let chain_output_entries: Vec<&OutputEntry> = resolved_outputs.iter().collect();
    let output_sigs: Vec<OutputStreamSignature> = if !chain_output_entries.is_empty() {
        chain_output_entries
            .iter()
            .zip(outputs.iter())
            .map(|(co, ro)| OutputStreamSignature {
                device_id: co.device_id.0.clone(),
                channels: co.channels.clone(),
                stream_channels: ro.supported.channels(),
                sample_rate: resolved_output_sample_rate(ro),
                buffer_size_frames: resolved_output_buffer_size_frames(ro),
            })
            .collect()
    } else {
        outputs
            .iter()
            .map(|ro| OutputStreamSignature {
                device_id: String::new(),
                channels: Vec::new(),
                stream_channels: ro.supported.channels(),
                sample_rate: resolved_output_sample_rate(ro),
                buffer_size_frames: resolved_output_buffer_size_frames(ro),
            })
            .collect()
    };

    ChainStreamSignature {
        inputs: input_sigs,
        outputs: output_sigs,
    }
}
