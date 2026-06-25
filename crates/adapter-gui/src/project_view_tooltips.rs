//! Chain input/output tooltip helpers — lifted out of project_view.rs
//! so the parent module stays under the size cap.

use domain::io_binding::IoBinding;
use infra_cpal::AudioDeviceDescriptor;
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;

use crate::project_view::format_channel_list;

pub(crate) fn chain_inputs_tooltip(
    chain: &Chain,
    _project: &Project,
    devices: &[AudioDeviceDescriptor],
    io_bindings: &[IoBinding],
) -> String {
    // #716: the device endpoints resolve from the binding registry, not from
    // block `entries` (which no longer exist on the model).
    let (resolved_inputs, _) = engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
    if resolved_inputs.is_empty() {
        return "No input configured".to_string();
    }
    resolved_inputs
        .iter()
        .enumerate()
        .map(|(ei, entry)| {
            let device_name = devices
                .iter()
                .find(|d| d.id == entry.device_id.0)
                .map(|d| d.name.as_str())
                .unwrap_or(&entry.device_id.0);
            let mode = match entry.mode {
                ChainInputMode::Mono => "Mono",
                ChainInputMode::Stereo => "Stereo",
                ChainInputMode::DualMono => "Dual Mono",
            };
            let label = rust_i18n::t!("label-input-numbered", n = ei + 1).to_string();
            format!(
                "{}: {} · {} · Ch {}",
                label,
                device_name,
                mode,
                format_channel_list(&entry.channels)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn chain_outputs_tooltip(
    chain: &Chain,
    _project: &Project,
    devices: &[AudioDeviceDescriptor],
    io_bindings: &[IoBinding],
) -> String {
    // #716: device endpoints resolve from the binding registry, not from
    // block `entries`.
    let (_, resolved_outputs) = engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
    if resolved_outputs.is_empty() {
        return "No output configured".to_string();
    }
    resolved_outputs
        .iter()
        .enumerate()
        .map(|(ei, entry)| {
            let device_name = devices
                .iter()
                .find(|d| d.id == entry.device_id.0)
                .map(|d| d.name.as_str())
                .unwrap_or(&entry.device_id.0);
            let mode = match entry.mode {
                ChainOutputMode::Mono => "Mono",
                ChainOutputMode::Stereo => "Stereo",
            };
            let label = rust_i18n::t!("label-output-numbered", n = ei + 1).to_string();
            format!(
                "{}: {} · {} · Ch {}",
                label,
                device_name,
                mode,
                format_channel_list(&entry.channels)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
