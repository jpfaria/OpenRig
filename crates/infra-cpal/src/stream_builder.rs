//! Build cpal `Stream`s for one chain (or for the project as a whole)
//! and assemble the per-chain `ActiveChainRuntime`.
//!
//! Public surface:
//!
//! - `build_streams_for_project` — used by callers that need every
//!   chain's streams in a flat `Vec<Stream>` for diagnostics or test
//!   harnesses; no-op on Linux+JACK because every byte goes through
//!   the JACK direct backend.
//!
//! Internal (`pub(crate)`) — the controller uses these, and they need
//! to live alongside `build_input/output_stream_for_input/output`
//! because those carry the audio-thread closures:
//!
//! - `build_input_stream_for_input` / `build_output_stream_for_output`
//!   — per-format match (F32 / I16 / U16 / I32) with a closure that
//!   the cpal callback runs on the audio thread.
//! - `build_chain_streams` — group the per-input + per-output streams.
//! - `build_active_chain_runtime` — the entry point: on Linux+JACK
//!   it routes to `build_jack_direct_chain`, otherwise it stitches
//!   together the cpal streams.
//! - `build_chain_stream_signature_multi` — derive the stream
//!   signature from a chain's resolved IO; used by `chain_resolve`.

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::anyhow;
use anyhow::Result;
use std::sync::Arc;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::{DeviceTrait, StreamTrait};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::SampleFormat;
use cpal::Stream;

use domain::ids::ChainId;
use engine::runtime::ChainRuntimeState;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::block::{InputEntry, OutputEntry};
use project::chain::Chain;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::callback_load_timing::record_callback_deadline;
use crate::active_runtime::ActiveChainRuntime;
use crate::resolved::ResolvedChainAudioConfig;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::{
    ChainStreamSignature, InputStreamSignature, OutputStreamSignature, ResolvedInputDevice,
    ResolvedOutputDevice,
};
use crate::LiveRuntimeSlot;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::{build_chain_slots, process_input_buffer, process_output_buffer};

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::stream_config::{
    build_stream_config, resolved_input_buffer_size_frames, resolved_input_sample_rate,
    resolved_output_buffer_size_frames, resolved_output_sample_rate,
};

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::host::jack_server_is_running;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_direct::build_jack_direct_chain;

pub fn build_streams_for_project(
    project: &project::project::Project,
    runtime_graph: &engine::runtime::RuntimeGraph,
) -> Result<Vec<Stream>> {
    log::info!("building audio streams for project");

    // On Linux with JACK, no CPAL streams are ever needed — streaming is handled
    // entirely by the jack crate in build_active_chain_runtime. Also, calling
    // validate_channels_against_devices() here would probe ALSA PCM and disturb
    // USB audio devices.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = project; // not needed on Linux/JACK
        let _ = runtime_graph; // not needed on Linux/JACK: all streaming handled by jack crate
        return Ok(Vec::new());
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = crate::host::get_host();
        crate::validation::validate_channels_against_devices(project, host)?;
        let mut resolved_chains =
            crate::chain_resolve::resolve_enabled_chain_audio_configs(host, project)?;
        let mut streams = Vec::new();
        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            // Issue #350 phase 3: a chain owns N per-input runtimes (one
            // per physical input device). Pass the full ordered
            // (group_id, runtime) list so each input cpal stream feeds
            // runtime (chain, group) and the output cpal stream mixes them
            // at the backend. Single-input chains have exactly one entry
            // here and take the byte-identical fast path.
            let runtimes = runtime_graph.runtimes_with_groups_for(&chain.id);
            if runtimes.is_empty() {
                return Err(anyhow!("chain '{}' has no runtime state", chain.id.0));
            }
            // This bulk/console path has no controller to hold the slots, so the
            // wrappers are throwaway (no live swap needed here); the streams
            // still read through them identically.
            let slots = build_chain_slots(&runtimes);
            let resolved = resolved_chains
                .remove(&chain.id)
                .ok_or_else(|| anyhow!("chain '{}' missing resolved audio config", chain.id.0))?;
            let (input_streams, output_streams) = build_chain_streams(&chain.id, resolved, slots)?;
            streams.extend(input_streams);
            streams.extend(output_streams);
        }
        Ok(streams)
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_input_stream_for_input(
    chain_id: &ChainId,
    input_index: usize,
    resolved_input_device: ResolvedInputDevice,
    slot: LiveRuntimeSlot,
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
    let stream = match sample_format {
        SampleFormat::F32 => {
            let slot_for_data = slot.handle();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| {
                    // Time the block DSP (the heavy work runs here, not on the
                    // output pop) so a buffer-64 deadline miss is counted as an
                    // xrun and surfaced instead of crackling silently.
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_buffer(&slot_for_data, input_index, data, channels);
                    }));
                    record_callback_deadline(
                        &slot_for_data.load(),
                        callback_start.elapsed(),
                        data.len() / channels,
                        sample_rate,
                    );
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I16 => {
            let slot_for_data = slot.handle();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i16::MAX as f32;
                    }
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_buffer(&slot_for_data, input_index, &converted, channels);
                    }));
                    record_callback_deadline(
                        &slot_for_data.load(),
                        callback_start.elapsed(),
                        converted.len() / channels,
                        sample_rate,
                    );
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::U16 => {
            let slot_for_data = slot.handle();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = (src as f32 / u16::MAX as f32) * 2.0 - 1.0;
                    }
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_buffer(&slot_for_data, input_index, &converted, channels);
                    }));
                    record_callback_deadline(
                        &slot_for_data.load(),
                        callback_start.elapsed(),
                        converted.len() / channels,
                        sample_rate,
                    );
                },
                move |err| log::error!("[{}] input stream error: {}", error_chain_id, err),
                None,
            )?
        }
        SampleFormat::I32 => {
            let slot_for_data = slot.handle();
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            let mut converted = Vec::new();
            device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| {
                    converted.resize(data.len(), 0.0);
                    for (dst, src) in converted.iter_mut().zip(data.iter().copied()) {
                        *dst = src as f32 / i32::MAX as f32;
                    }
                    let callback_start = std::time::Instant::now();
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        process_input_buffer(&slot_for_data, input_index, &converted, channels);
                    }));
                    record_callback_deadline(
                        &slot_for_data.load(),
                        callback_start.elapsed(),
                        converted.len() / channels,
                        sample_rate,
                    );
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
    let stream = match sample_format {
        SampleFormat::F32 => {
            let slots_for_data = slots.clone();
            let mut loaded: Vec<Arc<ChainRuntimeState>> = Vec::with_capacity(slots_for_data.len());
            let channels = stream_config.channels as usize;
            let error_chain_id = chain_id.0.clone();
            // Pre-allocated backend-mix scratch (issue #350 phase 3). Sized
            // to the configured buffer once here; the steady-state callback
            // never allocates. `process_output_f32_mixed` takes the
            // single-runtime byte-identical fast path when len()==1.
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [f32], _| {
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
            let mut temp = Vec::new();
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i16], _| {
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
            let mut temp = Vec::new();
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [u16], _| {
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
            let mut temp = Vec::new();
            let mut mix_scratch: Vec<f32> = vec![0.0; buffer_size_frames as usize * channels];
            device.build_output_stream(
                &stream_config,
                move |out: &mut [i32], _| {
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

/// Stitch the per-input + per-output cpal streams for one chain.
///
/// Issue #350 phase 3: `runtimes` is the chain's ordered list of
/// per-input runtimes — `(group_id, ChainRuntimeState)` where `group_id`
/// is the cpal input index that runtime owns (see
/// `RuntimeGraph::runtimes_with_groups_for`). The engine's
/// `effective_inputs` assigns cpal indices by first-seen distinct device
/// over the chain's raw input entries; `resolved.inputs` is in that same
/// raw-entry order, so deduplicating it by device in iteration order
/// yields the Nth distinct device == group N. Each physical input device
/// therefore gets its OWN cpal stream bound to its OWN runtime
/// `(chain, group)` — never collapsed to the first. The shared output
/// device's stream is handed EVERY runtime and sums them at the backend
/// (the only mix point invariant #4 permits).
#[cfg(not(all(target_os = "linux", feature = "jack")))]
fn build_chain_streams(
    chain_id: &ChainId,
    resolved: ResolvedChainAudioConfig,
    slots: Vec<(usize, LiveRuntimeSlot)>,
) -> Result<(Vec<Stream>, Vec<Stream>)> {
    // group_id -> live runtime slot, for binding each physical input device to
    // the isolated runtime that owns its cpal index. Issue #672: the callbacks
    // read the slot live so a worker-published rebuild takes effect without a
    // stream rebuild.
    let slot_by_group: std::collections::HashMap<usize, LiveRuntimeSlot> =
        slots.iter().map(|(g, s)| (*g, s.handle())).collect();
    // Flat list (group order) for the backend output mix.
    let all_slots: Vec<LiveRuntimeSlot> = slots.iter().map(|(_, s)| s.handle()).collect();
    // Fallback used only if a chain somehow has no per-input runtime for a
    // given group (degenerate config) — keeps behaviour defined instead of
    // panicking on the audio-setup path.
    let first_slot = all_slots.first().cloned();

    // Deduplicate input streams by device: one CPAL stream per unique
    // device. Iteration order over resolved.inputs matches the engine's
    // first-seen-device cpal-index assignment, so the Nth distinct device
    // is group N and binds to runtime (chain, N).
    let mut input_streams = Vec::new();
    let mut seen_devices: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut next_group: usize = 0;
    for (i, resolved_input) in resolved.inputs.into_iter().enumerate() {
        let device_key = resolved_input
            .device
            .id()
            .map(|id| id.to_string())
            .unwrap_or_default();
        if !seen_devices.insert(device_key.clone()) {
            log::info!(
                "input[{}] shares device '{}', reusing existing CPAL stream",
                i,
                device_key
            );
            continue;
        }
        let group = next_group;
        next_group += 1;
        let slot = slot_by_group
            .get(&group)
            .cloned()
            .or_else(|| first_slot.clone())
            .ok_or_else(|| {
                anyhow!(
                    "chain '{}' input group {} has no per-input runtime",
                    chain_id.0,
                    group
                )
            })?;
        let stream = build_input_stream_for_input(chain_id, group, resolved_input, slot)?;
        input_streams.push(stream);
    }

    let mut output_streams = Vec::new();
    for (j, resolved_output) in resolved.outputs.into_iter().enumerate() {
        let stream =
            build_output_stream_for_output(chain_id, j, resolved_output, all_slots.clone())?;
        output_streams.push(stream);
    }

    Ok((input_streams, output_streams))
}

/// Build (and start) the cpal streams for one chain. Issue #350 phase 3:
/// `runtimes` is the chain's full ordered `(group_id, runtime)` list — the
/// cpal path wires each physical input device to its own runtime and the
/// shared output device sums them. The Linux/JACK path is unchanged: it
/// keeps the single-runtime model (Insert / JACK-direct chains are one
/// runtime by Phase-1 design) and uses the first runtime.
pub(crate) fn build_active_chain_runtime(
    chain_id: &ChainId,
    #[allow(unused_variables)] chain: &Chain,
    resolved: ResolvedChainAudioConfig,
    slots: Vec<(usize, LiveRuntimeSlot)>,
) -> Result<ActiveChainRuntime> {
    log::info!(
        "building active chain runtime for '{}', sample_rate={}",
        chain_id.0,
        resolved.sample_rate
    );
    let stream_signature = resolved.stream_signature.clone();

    // On Linux with JACK: use the jack crate directly for zero-overhead audio.
    // This bypasses CPAL entirely — the JACK process callback runs in the
    // real-time thread with no extra buffering.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        if jack_server_is_running() {
            log::info!("JACK detected — using direct JACK backend (bypassing CPAL)");
            // JACK-direct chains are a single runtime by Phase-1 design
            // (Insert pipelines are not partitioned). Use the first.
            // JACK does not yet read through the live slot (issue #672 wires the
            // cpal path first); load the published runtime once here so JACK keeps
            // its current behaviour. Live JACK swap is a follow-up.
            let runtime = slots
                .into_iter()
                .next()
                .map(|(_, slot)| slot.load())
                .ok_or_else(|| anyhow::anyhow!("chain '{}' has no runtime state", chain_id.0))?;
            let (jack_client, dsp_worker) = build_jack_direct_chain(chain_id, chain, runtime)?;
            return Ok(ActiveChainRuntime {
                stream_signature,
                _input_streams: Vec::new(),
                _output_streams: Vec::new(),
                _jack_client: Some(jack_client),
                _dsp_worker: Some(dsp_worker),
            });
        }
        // JACK not running on Linux+JACK build — return an empty
        // ActiveChainRuntime; resolved.inputs/outputs are empty in this
        // mode. Matches the pre-split behaviour where the function fell
        // through to the CPAL path with nothing to build.
        let _ = chain_id;
        let _ = resolved;
        let _ = slots;
        return Ok(ActiveChainRuntime {
            stream_signature,
            _input_streams: Vec::new(),
            _output_streams: Vec::new(),
            _jack_client: None,
            _dsp_worker: None,
        });
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let (input_streams, output_streams) = build_chain_streams(chain_id, resolved, slots)?;
        for stream in &input_streams {
            stream.play()?;
        }
        for stream in &output_streams {
            stream.play()?;
        }
        log::info!(
            "audio streams started for chain '{}': {} input(s), {} output(s)",
            chain_id.0,
            input_streams.len(),
            output_streams.len()
        );
        Ok(ActiveChainRuntime {
            stream_signature,
            _input_streams: input_streams,
            _output_streams: output_streams,
        })
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn build_chain_stream_signature_multi(
    chain: &Chain,
    inputs: &[ResolvedInputDevice],
    outputs: &[ResolvedOutputDevice],
) -> ChainStreamSignature {
    let chain_input_entries: Vec<&InputEntry> = chain
        .input_blocks()
        .into_iter()
        .flat_map(|(_, ib)| ib.entries.iter())
        .collect();
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

    let chain_output_entries: Vec<&OutputEntry> = chain
        .output_blocks()
        .into_iter()
        .flat_map(|(_, ob)| ob.entries.iter())
        .collect();
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
