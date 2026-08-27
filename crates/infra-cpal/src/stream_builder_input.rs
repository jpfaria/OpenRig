//! Responsibility: builds the cpal input stream for one physical input device.
//!
//! Every supported sample format (F32 / I16 / U16 / I32) gets the same
//! shape: convert to `f32` when needed, then hand the buffer to each
//! runtime slot this device feeds. On macOS the DSP is moved off the
//! callback into a per-slot `dsp_worker` (see the F32 arm).

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::Result;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::DeviceTrait;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::SampleFormat;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::Stream;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use domain::ids::ChainId;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::callback_load_timing::record_callback_deadline;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::process_input_buffer;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::ResolvedInputDevice;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::stream_config::{
    build_stream_config, resolved_input_buffer_size_frames, resolved_input_sample_rate,
};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::LiveRuntimeSlot;

/// Issue #703: `slots` is every per-entry runtime this device's single
/// stream feeds (two input entries on one interface are two isolated
/// runtimes sharing the stream). The callback fans the buffer out to each
/// slot; one-slot chains keep the historical single-runtime shape.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_input_stream_for_input(
    chain_id: &ChainId,
    input_index: usize,
    resolved_input_device: ResolvedInputDevice,
    slots: Vec<LiveRuntimeSlot>,
) -> Result<Stream> {
    log::debug!(
        "building input stream for chain '{}' input_index={}",
        chain_id.0,
        input_index
    );
    let sample_format = resolved_input_device.supported.sample_format();
    let sample_rate = resolved_input_sample_rate(&resolved_input_device);
    let buffer_size_frames = resolved_input_buffer_size_frames(&resolved_input_device);
    log::debug!(
        "input stream config: chain='{}', input_index={}, sample_rate={}, buffer_size={}, format={:?}, channels={}",
        chain_id.0, input_index, sample_rate, buffer_size_frames, sample_format, resolved_input_device.supported.channels()
    );
    let stream_config = build_stream_config(
        resolved_input_device.supported.channels(),
        sample_rate,
        buffer_size_frames,
    );
    let device = resolved_input_device.device;
    // #760: the workgroup join must target THIS device (resolved off the audio
    // thread), not the system default — otherwise the non-default interface's
    // callback co-schedules with the wrong device and underruns under load.
    let workgroup_uid = device.id().ok().map(|id| id.to_string());
    let stream = match sample_format {
        SampleFormat::F32 if cfg!(target_os = "macos") => {
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            // #670 (macOS ONLY — cross-platform law: a fix for one OS stays
            // behind a guard): the chain DSP does NOT run in this callback.
            // The HAL thread sleeps between cycles, the NAM working set
            // cools, and the cold-cache inference tail sporadically crossed
            // the cycle (CoreAudio then drops input — the click). The
            // callback only copies the buffer into the worker's lock-free
            // ring (microseconds); the per-stream dsp_worker runs the chain.
            // Windows / Linux-cpal keep the proven inline path below: the
            // worker's realtime promotion is mach-specific, and an
            // UNPROMOTED busy worker measured catastrophically worse
            // (167 xruns + 256 underruns per 60 s) than inline DSP.
            // Issue #703: one worker PER per-entry runtime — each entry's
            // chain DSP runs on its own realtime thread, so a heavy entry
            // cannot starve its sibling sharing this device stream.
            let workers: Vec<_> = slots
                .iter()
                .enumerate()
                .map(|(k, slot)| {
                    crate::dsp_worker::spawn(
                        format!("{}:{input_index}:{k}", chain_id.0),
                        slot.handle(),
                        input_index,
                        channels,
                        sample_rate,
                        (buffer_size_frames as usize).max(64) * channels * 8,
                        workgroup_uid.clone(),
                    )
                })
                .collect();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    // #670: co-schedule this callback thread with the audio I/O
                    // workgroup so its cache (NAM weights) stays warm.
                    crate::audio_workgroup::ensure_joined_input(workgroup_uid.as_deref());
                    for worker in &workers {
                        worker.push(data);
                    }
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::F32 => {
            let slots_for_data: Vec<LiveRuntimeSlot> = slots.iter().map(|s| s.handle()).collect();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    // Inline DSP (non-macOS cpal path; see the macOS arm above).
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        for slot in &slots_for_data {
                            process_input_buffer(slot, input_index, data, channels);
                        }
                    }));
                    let elapsed = callback_start.elapsed();
                    for slot in &slots_for_data {
                        record_callback_deadline(
                            &slot.load(),
                            elapsed,
                            data.len() / channels,
                            sample_rate,
                        );
                    }
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let slots_for_data: Vec<LiveRuntimeSlot> = slots.iter().map(|s| s.handle()).collect();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    crate::audio_workgroup::ensure_joined_input(workgroup_uid.as_deref());
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i16::MAX as f32;
                    }
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        for slot in &slots_for_data {
                            process_input_buffer(slot, input_index, &converted, channels);
                        }
                    }));
                    let elapsed = callback_start.elapsed();
                    for slot in &slots_for_data {
                        record_callback_deadline(
                            &slot.load(),
                            elapsed,
                            converted.len() / channels,
                            sample_rate,
                        );
                    }
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let slots_for_data: Vec<LiveRuntimeSlot> = slots.iter().map(|s| s.handle()).collect();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    crate::audio_workgroup::ensure_joined_input(workgroup_uid.as_deref());
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = (src as f32 / u16::MAX as f32) * 2.0 - 1.0;
                    }
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        for slot in &slots_for_data {
                            process_input_buffer(slot, input_index, &converted, channels);
                        }
                    }));
                    let elapsed = callback_start.elapsed();
                    for slot in &slots_for_data {
                        record_callback_deadline(
                            &slot.load(),
                            elapsed,
                            converted.len() / channels,
                            sample_rate,
                        );
                    }
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let slots_for_data: Vec<LiveRuntimeSlot> = slots.iter().map(|s| s.handle()).collect();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| {
                    crate::audio_workgroup::ensure_joined_input(workgroup_uid.as_deref());
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i32::MAX as f32;
                    }
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        for slot in &slots_for_data {
                            process_input_buffer(slot, input_index, &converted, channels);
                        }
                    }));
                    let elapsed = callback_start.elapsed();
                    for slot in &slots_for_data {
                        record_callback_deadline(
                            &slot.load(),
                            elapsed,
                            converted.len() / channels,
                            sample_rate,
                        );
                    }
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        other => {
            anyhow::bail!(
                "unsupported input sample format for chain '{}': {:?}",
                chain_id.0,
                other
            );
        }
    };
    Ok(stream)
}
