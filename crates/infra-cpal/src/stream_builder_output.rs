//! Responsibility: builds the cpal output stream for one physical output device.
//!
//! The stream is handed every runtime slot the chain owns and sums them
//! at the backend — the only mix point CLAUDE.md invariant #4 permits.
//! The mix scratch buffer is allocated here, at build time, so the audio
//! thread allocates nothing.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::Result;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use std::sync::Arc;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::DeviceTrait;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::SampleFormat;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::Stream;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use domain::ids::ChainId;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use engine::runtime::ChainRuntimeState;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::process_output_buffer;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::ResolvedOutputDevice;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::stream_config::{
    build_stream_config, resolved_output_buffer_size_frames, resolved_output_sample_rate,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::LiveRuntimeSlot;

/// Build the cpal output stream for one physical output device. Issue
/// #350 phase 3: a chain may own N per-input runtimes (one isolated
/// `ChainRuntimeState` per physical input device). This single physical
/// output device must SUM all of them — the backend mix CLAUDE.md
/// invariant #4 mandates (each runtime's SPSC ring still has exactly one
/// producer and is consumed once here). The `scratch` mix buffer is
/// pre-allocated here at stream-build time and reused every callback so
/// the audio thread allocates nothing. Single-runtime chains (99% case)
/// hit the byte-identical fast path inside `process_output_f32_mixed`.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_output_stream_for_output(
    chain_id: &ChainId,
    output_index: usize,
    resolved_output_device: ResolvedOutputDevice,
    slots: Vec<LiveRuntimeSlot>,
    di_cell: crate::di_playback::DiPlaybackCell,
) -> Result<Stream> {
    log::debug!(
        "building output stream for chain '{}' output_index={}",
        chain_id.0,
        output_index
    );
    let sample_format = resolved_output_device.supported.sample_format();
    let sample_rate = resolved_output_sample_rate(&resolved_output_device);
    let buffer_size_frames = resolved_output_buffer_size_frames(&resolved_output_device);
    log::debug!(
        "output stream config: chain='{}', output_index={}, sample_rate={}, buffer_size={}, format={:?}, channels={}",
        chain_id.0, output_index, sample_rate, buffer_size_frames, sample_format, resolved_output_device.supported.channels()
    );
    let stream_config = build_stream_config(
        resolved_output_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_output_device.device;
    // #760: join THIS output device's workgroup, not the system default.
    let workgroup_uid = device.id().ok().map(|id| id.to_string());
    let stream = match sample_format {
        SampleFormat::F32 => {
            let slots_for_data = slots.clone();
            let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(slots_for_data.len());
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let di_cell = di_cell.clone();
            // Pre-allocated backend-mix scratch (issue #350 phase 3). Sized
            // to the configured buffer once here; the steady-state callback
            // never allocates. `process_output_f32_mixed` takes the
            // single-runtime byte-identical fast path when len()==1.
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
                    crate::audio_workgroup::ensure_joined_output(workgroup_uid.as_deref());
                    if mix_scratch.len() < out.len() {
                        mix_scratch.resize(out.len(), 0.0);
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_buffer(
                            &slots_for_data,
                            &mut loaded,
                            output_index,
                            out,
                            channels,
                            &mut mix_scratch,
                        );
                        crate::di_playback::mix_di_playback(&di_cell, out, channels);
                    }));
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let slots_for_data = slots.clone();
            let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(slots_for_data.len());
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let di_cell = di_cell.clone();
            let mut temp: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
                    crate::audio_workgroup::ensure_joined_output(workgroup_uid.as_deref());
                    temp.resize(out.len(), 0.0);
                    if mix_scratch.len() < out.len() {
                        mix_scratch.resize(out.len(), 0.0);
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_buffer(
                            &slots_for_data,
                            &mut loaded,
                            output_index,
                            &mut temp,
                            channels,
                            &mut mix_scratch,
                        );
                        crate::di_playback::mix_di_playback(&di_cell, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst =
                            (*src * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let slots_for_data = slots.clone();
            let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(slots_for_data.len());
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let di_cell = di_cell.clone();
            let mut temp: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
                    crate::audio_workgroup::ensure_joined_output(workgroup_uid.as_deref());
                    temp.resize(out.len(), 0.0);
                    if mix_scratch.len() < out.len() {
                        mix_scratch.resize(out.len(), 0.0);
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_buffer(
                            &slots_for_data,
                            &mut loaded,
                            output_index,
                            &mut temp,
                            channels,
                            &mut mix_scratch,
                        );
                        crate::di_playback::mix_di_playback(&di_cell, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        let normalized =
                            ((*src + 1.0) * 0.5 * u16::MAX as f32).clamp(0.0, u16::MAX as f32);
                        *dst = normalized as u16;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let slots_for_data = slots.clone();
            let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(slots_for_data.len());
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let di_cell = di_cell.clone();
            let mut temp: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i32], _| {
                    crate::audio_workgroup::ensure_joined_output(workgroup_uid.as_deref());
                    temp.resize(out.len(), 0.0);
                    if mix_scratch.len() < out.len() {
                        mix_scratch.resize(out.len(), 0.0);
                    }
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_output_buffer(
                            &slots_for_data,
                            &mut loaded,
                            output_index,
                            &mut temp,
                            channels,
                            &mut mix_scratch,
                        );
                        crate::di_playback::mix_di_playback(&di_cell, &mut temp, channels);
                    }));
                    for (dst, src) in out.iter_mut().zip(temp.iter()) {
                        *dst =
                            (*src * i32::MAX as f32).clamp(i32::MIN as f32, i32::MAX as f32) as i32;
                    }
                },
                move |err| log::error!("[{}] output stream error: {}", error_chain_id, err),
                None,
            )?
        }
        other => {
            anyhow::bail!(
                "unsupported output sample format for chain '{}': {:?}",
                chain_id.0,
                other
            );
        }
    };
    Ok(stream)
}
