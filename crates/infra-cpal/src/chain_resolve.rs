//! Translate a `project::Chain` + `DeviceSettings` into a fully-resolved
//! audio config (`ResolvedChainAudioConfig`).
//!
//! This is the boundary between the project's logical view ("device_id
//! 'jack:gen', channels [0,1]") and what cpal can actually open
//! (`cpal::Device` + `SupportedStreamConfig`). The Linux+JACK path
//! short-circuits before reaching here — `sync_project_jack_direct`
//! builds its synthetic config in `streams_project::jack_resolve_chain_config`
//! using libjack data, never ALSA.
//!
//! Public surface:
//! - `resolve_project_chain_sample_rates` — used by the application layer
//!   to pre-compute per-chain sample rates (UI display + engine init).
//!
//! Internal helpers (all `pub(crate)` — consumed by `controller.rs`):
//! - `resolve_chain_audio_config`,
//! - `resolve_enabled_chain_audio_configs`,
//! - `resolve_chain_inputs` / `resolve_chain_outputs` (also feed
//!   `resolve_project_chain_sample_rates`),
//! - `resolve_input_device_for_chain_input` /
//!   `resolve_output_device_for_chain_output`,
//! - `insert_return_as_input_entry` / `insert_send_as_output_entry`
//!   (the InsertBlock endpoint adapters).

use anyhow::Result;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::{bail, Context};
use std::collections::HashMap;

use domain::ids::ChainId;
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use domain::ids::DeviceId;
use domain::io_binding::IoBinding;
use project::project::Project;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use engine::runtime_endpoints::{resolve_chain_io, resolve_chain_io_by_binding, InputEntry, OutputEntry};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::block::{AudioBlockKind, InsertBlock};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::chain::Chain;


#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::{is_asio_host, using_jack_direct};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::resolved::{
    ChainStreamSignature, ResolvedChainAudioConfig, ResolvedInputDevice, ResolvedOutputDevice,
};

#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::jack_supervisor;
#[cfg(all(target_os = "linux", feature = "jack"))]
use crate::usb_proc::{detect_all_usb_audio_cards, jack_server_is_running_for};
#[cfg(all(target_os = "linux", feature = "jack"))]
use anyhow::anyhow;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use anyhow::anyhow;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use crate::host::get_host;

pub fn resolve_project_chain_sample_rates(
    project: &Project,
    registry: &[IoBinding],
) -> Result<HashMap<ChainId, f32>> {
    // On Linux+JACK, get sample rate from JACK server directly — zero ALSA access.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
        let _ = registry; // device endpoints come from libjack meta on this path
        // Probe the first running named server via the libjack helper — no
        // cache involved; this is a one-off read for UI/display purposes.
        let cards = detect_all_usb_audio_cards();
        let meta = cards
            .iter()
            .find(|c| jack_server_is_running_for(&c.server_name))
            .map(|c| jack_supervisor::ServerName::from(c.server_name.clone()))
            .ok_or_else(|| anyhow!("no running JACK server found"))
            .and_then(|name| jack_supervisor::live_backend::probe_server_meta(&name))?;
        let sr = meta.sample_rate as f32;
        let mut sample_rates = HashMap::new();
        for chain in &project.chains {
            if chain.enabled {
                sample_rates.insert(chain.id.clone(), sr);
            }
        }
        return Ok(sample_rates);
    }

    #[cfg(not(all(target_os = "linux", feature = "jack")))]
    {
        let host = get_host();
        let mut sample_rates = HashMap::new();

        for chain in &project.chains {
            if !chain.enabled {
                continue;
            }
            let inputs = resolve_chain_inputs(host, project, chain, registry)?;
            let outputs = resolve_chain_outputs(host, project, chain, registry)?;
            let (logical_inputs, logical_outputs) =
                engine::runtime_endpoints::resolve_chain_io(chain, registry);
            let mut by_device: std::collections::HashMap<domain::ids::DeviceId, u32> =
                std::collections::HashMap::new();
            for (logical, resolved) in logical_inputs.iter().zip(inputs.iter()) {
                by_device.insert(logical.device_id.clone(), crate::resolved_input_sample_rate(resolved));
            }
            for (logical, resolved) in logical_outputs.iter().zip(outputs.iter()) {
                by_device.insert(logical.device_id.clone(), crate::resolved_output_sample_rate(resolved));
            }
            let binding_rates: Vec<(Vec<u32>, Vec<u32>)> =
                engine::runtime_endpoints::resolve_chain_io_by_binding(chain, registry)
                    .iter()
                    .map(|g| {
                        let in_r = g.inputs.iter().map(|e| by_device.get(&e.device_id).copied().unwrap_or(0)).collect();
                        let out_r = g.outputs.iter().map(|e| by_device.get(&e.device_id).copied().unwrap_or(0)).collect();
                        (in_r, out_r)
                    })
                    .collect();
            let sample_rate = crate::resolve_binding_sample_rates(&chain.id.0, &binding_rates)?;
            sample_rates.insert(chain.id.clone(), sample_rate);
        }

        Ok(sample_rates)
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_input_device_for_chain_input(
    host: &cpal::Host,
    project: &Project,
    input: &InputEntry,
    is_asio: bool,
) -> Result<ResolvedInputDevice> {
    let settings = project
        .device_settings
        .iter()
        .find(|s| s.device_id == input.device_id)
        .cloned();
    if using_jack_direct() {
        // Unreachable in JACK-direct mode: sync_project / upsert_chain short-circuit
        // into sync_project_jack_direct() before ever calling this function. If we
        // ever land here while JACK is active, something bypassed the short-circuit
        // and is about to probe ALSA on a device JACK owns — refuse instead.
        bail!("internal error: resolve_input_device_for_chain_input called in JACK-direct mode");
    }
    let device = crate::find_input_device_by_id(host, &input.device_id.0)?.ok_or_else(|| {
        anyhow!(
            "input device '{}' not found by device_id",
            input.device_id.0
        )
    })?;
    // #762: cached CoreAudio query — avoid re-probing the same device (and
    // disturbing it) on every live sync.
    let cfg = crate::device_config_cache::configs_for(&device, true).with_context(|| {
        format!("failed to query input configs for '{}'", input.device_id.0)
    })?;
    let default_config = cfg.default.ok_or_else(|| {
        anyhow!(
            "failed to get default input config for '{}'",
            input.device_id.0
        )
    })?;
    let supported_ranges = cfg.supported;
    let required_channels = crate::required_channel_count(&input.channels);
    let supported = crate::select_supported_stream_config(
        &default_config,
        &supported_ranges,
        settings.as_ref().map(|s| s.sample_rate),
        required_channels,
        &input.device_id.0,
    )?;
    // For ASIO, skip buffer size range validation — the project's requested buffer size
    // is passed directly to the ASIO driver via BufferSize::Fixed. The driver accepts or
    // rejects it at stream build time with a real error. Pre-validation is incorrect for
    // ASIO because the driver's reported range reflects its current preferred size, not
    // what it actually accepts when asked.
    if !is_asio {
        if let Some(settings) = &settings {
            crate::validate_buffer_size(
                settings.buffer_size_frames,
                supported.buffer_size(),
                &settings.device_id.0,
            )?;
        }
    }
    Ok(ResolvedInputDevice {
        settings,
        device,
        supported,
    })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_output_device_for_chain_output(
    host: &cpal::Host,
    project: &Project,
    output: &OutputEntry,
    is_asio: bool,
) -> Result<ResolvedOutputDevice> {
    let settings = project
        .device_settings
        .iter()
        .find(|s| s.device_id == output.device_id)
        .cloned();
    if using_jack_direct() {
        // Unreachable in JACK-direct mode (see matching guard in the input path).
        bail!("internal error: resolve_output_device_for_chain_output called in JACK-direct mode");
    }
    let device = crate::find_output_device_by_id(host, &output.device_id.0)?.ok_or_else(|| {
        anyhow!(
            "output device '{}' not found by device_id",
            output.device_id.0
        )
    })?;
    // #762: cached CoreAudio query — avoid re-probing on every live sync.
    let cfg = crate::device_config_cache::configs_for(&device, false).with_context(|| {
        format!("failed to query output configs for '{}'", output.device_id.0)
    })?;
    let default_config = cfg.default.ok_or_else(|| {
        anyhow!(
            "failed to get default output config for '{}'",
            output.device_id.0
        )
    })?;
    let supported_ranges = cfg.supported;
    let required_channels = crate::required_channel_count(&output.channels);
    let supported = crate::select_supported_stream_config(
        &default_config,
        &supported_ranges,
        settings.as_ref().map(|s| s.sample_rate),
        required_channels,
        &output.device_id.0,
    )?;
    if !is_asio {
        if let Some(settings) = &settings {
            crate::validate_buffer_size(
                settings.buffer_size_frames,
                supported.buffer_size(),
                &settings.device_id.0,
            )?;
        }
    }
    Ok(ResolvedOutputDevice {
        settings,
        device,
        supported,
    })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_chain_inputs(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
    registry: &[IoBinding],
) -> Result<Vec<ResolvedInputDevice>> {
    let is_asio = is_asio_host(host);
    // Model A (#716): device endpoints come from the binding registry, not from
    // block `entries`. `resolve_chain_io` yields head (io_binding_ids) + mid
    // Input blocks in order; Insert returns are appended below as before.
    let resolved_inputs = resolve_chain_io(chain, registry).0;
    let mut input_entries: Vec<&InputEntry> = resolved_inputs.iter().collect();
    // Include Insert block return endpoints as input streams
    let insert_return_entries: Vec<InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => insert_return_as_input_entry(ib, registry),
            _ => None,
        })
        .collect();
    let insert_refs: Vec<&InputEntry> = insert_return_entries.iter().collect();
    input_entries.extend(insert_refs);
    if input_entries.is_empty() {
        bail!("chain '{}' has no input blocks configured", chain.id.0);
    }
    input_entries
        .iter()
        .map(|input| resolve_input_device_for_chain_input(host, project, input, is_asio))
        .collect()
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_chain_outputs(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
    registry: &[IoBinding],
) -> Result<Vec<ResolvedOutputDevice>> {
    let is_asio = is_asio_host(host);
    // Model A (#716): output endpoints come from the binding registry (tail +
    // mid Output blocks), not from block `entries`. Insert sends appended below.
    let resolved_outputs = resolve_chain_io(chain, registry).1;
    let mut output_entries: Vec<&OutputEntry> = resolved_outputs.iter().collect();
    // Include Insert block send endpoints as output streams
    let insert_send_entries: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => insert_send_as_output_entry(ib, registry),
            _ => None,
        })
        .collect();
    let insert_refs: Vec<&OutputEntry> = insert_send_entries.iter().collect();
    output_entries.extend(insert_refs);
    if output_entries.is_empty() {
        bail!("chain '{}' has no output blocks configured", chain.id.0);
    }
    output_entries
        .iter()
        .map(|output| resolve_output_device_for_chain_output(host, project, output, is_asio))
        .collect()
}

/// Resolve an InsertBlock's RETURN to an InputEntry — model A (#716): the return
/// comes from the insert binding's INPUT endpoint in the registry. `None` when
/// the binding is absent or has no input endpoint.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn insert_return_as_input_entry(
    insert: &InsertBlock,
    registry: &[IoBinding],
) -> Option<InputEntry> {
    let binding = registry.iter().find(|b| b.id == insert.io)?;
    let ep = binding.inputs.first()?;
    Some(InputEntry {
        device_id: ep.device_id.clone(),
        mode: project::chain::ChainInputMode::from(ep.mode),
        channels: ep.channels.clone(),
    })
}

/// Resolve an InsertBlock's SEND to an OutputEntry — model A (#716): the send
/// goes to the insert binding's OUTPUT endpoint in the registry. `None` when the
/// binding is absent or has no output endpoint.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn insert_send_as_output_entry(
    insert: &InsertBlock,
    registry: &[IoBinding],
) -> Option<OutputEntry> {
    use project::chain::ChainOutputMode;
    let binding = registry.iter().find(|b| b.id == insert.io)?;
    let ep = binding.outputs.first()?;
    Some(OutputEntry {
        device_id: ep.device_id.clone(),
        mode: ChainOutputMode::try_from(ep.mode).unwrap_or(ChainOutputMode::Stereo),
        channels: ep.channels.clone(),
    })
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_enabled_chain_audio_configs(
    host: &cpal::Host,
    project: &Project,
    registry: &[IoBinding],
) -> Result<HashMap<ChainId, ResolvedChainAudioConfig>> {
    let mut resolved = HashMap::new();

    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }

        let config = resolve_chain_audio_config(host, project, chain, registry)?;
        resolved.insert(chain.id.clone(), config);
    }

    Ok(resolved)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_chain_audio_config(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
    registry: &[IoBinding],
) -> Result<ResolvedChainAudioConfig> {
    let inputs = resolve_chain_inputs(host, project, chain, registry)?;
    let outputs = resolve_chain_outputs(host, project, chain, registry)?;

    // #736: map each resolved device to its own rate. The resolved input /
    // output lists are in the same order as the logical endpoints from
    // `resolve_chain_io`, so we zip to recover each device id.
    let (logical_inputs, logical_outputs) = resolve_chain_io(chain, registry);
    let mut by_device: HashMap<DeviceId, f32> = HashMap::new();
    for (logical, resolved) in logical_inputs.iter().zip(inputs.iter()) {
        by_device.insert(
            logical.device_id.clone(),
            crate::resolved_input_sample_rate(resolved) as f32,
        );
    }
    for (logical, resolved) in logical_outputs.iter().zip(outputs.iter()) {
        by_device.insert(
            logical.device_id.clone(),
            crate::resolved_output_sample_rate(resolved) as f32,
        );
    }

    // #736: validate per binding (input==output within a binding) and allow
    // different rates across bindings, instead of one whole-chain unify.
    let binding_rates: Vec<(Vec<u32>, Vec<u32>)> = resolve_chain_io_by_binding(chain, registry)
        .iter()
        .map(|g| {
            let in_r = g
                .inputs
                .iter()
                .map(|e| by_device.get(&e.device_id).copied().unwrap_or(0.0) as u32)
                .collect();
            let out_r = g
                .outputs
                .iter()
                .map(|e| by_device.get(&e.device_id).copied().unwrap_or(0.0) as u32)
                .collect();
            (in_r, out_r)
        })
        .collect();
    let sample_rate = crate::resolve_binding_sample_rates(&chain.id.0, &binding_rates)?;

    let stream_signature: ChainStreamSignature =
        crate::build_chain_stream_signature_multi(chain, &inputs, &outputs, registry);

    Ok(ResolvedChainAudioConfig {
        inputs,
        outputs,
        sample_rate,
        by_device,
        stream_signature,
    })
}
