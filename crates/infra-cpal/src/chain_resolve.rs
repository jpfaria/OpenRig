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
use project::project::Project;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::block::{AudioBlockKind, InputEntry, InsertBlock, OutputEntry};
#[cfg(not(all(target_os = "linux", feature = "jack")))]
use project::chain::Chain;

#[cfg(not(all(target_os = "linux", feature = "jack")))]
use cpal::traits::DeviceTrait;

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

pub fn resolve_project_chain_sample_rates(project: &Project) -> Result<HashMap<ChainId, f32>> {
    // On Linux+JACK, get sample rate from JACK server directly — zero ALSA access.
    #[cfg(all(target_os = "linux", feature = "jack"))]
    {
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
            let inputs = resolve_chain_inputs(&host, project, chain)?;
            let outputs = resolve_chain_outputs(&host, project, chain)?;
            let sample_rate = crate::resolve_multi_io_sample_rate(&chain.id.0, &inputs, &outputs)?;
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
    let default_config = device.default_input_config().with_context(|| {
        format!(
            "failed to get default input config for '{}'",
            input.device_id.0
        )
    })?;
    let supported_ranges = device
        .supported_input_configs()
        .with_context(|| {
            format!(
                "failed to enumerate input configs for '{}'",
                input.device_id.0
            )
        })?
        .collect::<Vec<_>>();
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
    let default_config = device.default_output_config().with_context(|| {
        format!(
            "failed to get default output config for '{}'",
            output.device_id.0
        )
    })?;
    let supported_ranges = device
        .supported_output_configs()
        .with_context(|| {
            format!(
                "failed to enumerate output configs for '{}'",
                output.device_id.0
            )
        })?
        .collect::<Vec<_>>();
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
) -> Result<Vec<ResolvedInputDevice>> {
    let is_asio = is_asio_host(host);
    let mut input_entries: Vec<&InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Input(ib) => Some(ib),
            _ => None,
        })
        .flat_map(|ib| ib.entries.iter())
        .collect();
    // Include Insert block return endpoints as input streams
    let insert_return_entries: Vec<InputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_return_as_input_entry(ib)),
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
) -> Result<Vec<ResolvedOutputDevice>> {
    let is_asio = is_asio_host(host);
    let mut output_entries: Vec<&OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Output(ob) => Some(ob),
            _ => None,
        })
        .flat_map(|ob| ob.entries.iter())
        .collect();
    // Include Insert block send endpoints as output streams
    let insert_send_entries: Vec<OutputEntry> = chain
        .blocks
        .iter()
        .filter(|b| b.enabled)
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Insert(ib) => Some(insert_send_as_output_entry(ib)),
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

/// Convert an InsertBlock's return endpoint to an InputEntry for stream resolution.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn insert_return_as_input_entry(insert: &InsertBlock) -> InputEntry {
    InputEntry {
        device_id: insert.return_.device_id.clone(),
        mode: insert.return_.mode,
        channels: insert.return_.channels.clone(),
    }
}

/// Convert an InsertBlock's send endpoint to an OutputEntry for stream resolution.
#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn insert_send_as_output_entry(insert: &InsertBlock) -> OutputEntry {
    use project::chain::ChainOutputMode;
    OutputEntry {
        device_id: insert.send.device_id.clone(),
        mode: match insert.send.mode {
            project::chain::ChainInputMode::Mono => ChainOutputMode::Mono,
            _ => ChainOutputMode::Stereo,
        },
        channels: insert.send.channels.clone(),
    }
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_enabled_chain_audio_configs(
    host: &cpal::Host,
    project: &Project,
) -> Result<HashMap<ChainId, ResolvedChainAudioConfig>> {
    let mut resolved = HashMap::new();

    for chain in &project.chains {
        if !chain.enabled {
            continue;
        }

        let config = resolve_chain_audio_config(host, project, chain)?;
        resolved.insert(chain.id.clone(), config);
    }

    Ok(resolved)
}

#[cfg(not(all(target_os = "linux", feature = "jack")))]
pub(crate) fn resolve_chain_audio_config(
    host: &cpal::Host,
    project: &Project,
    chain: &Chain,
) -> Result<ResolvedChainAudioConfig> {
    let inputs = resolve_chain_inputs(host, project, chain)?;
    let outputs = resolve_chain_outputs(host, project, chain)?;

    // Validate sample rates: all inputs and outputs must agree
    let sample_rate = crate::resolve_multi_io_sample_rate(&chain.id.0, &inputs, &outputs)?;

    let stream_signature: ChainStreamSignature =
        crate::build_chain_stream_signature_multi(chain, &inputs, &outputs);

    Ok(ResolvedChainAudioConfig {
        inputs,
        outputs,
        sample_rate,
        stream_signature,
    })
}
