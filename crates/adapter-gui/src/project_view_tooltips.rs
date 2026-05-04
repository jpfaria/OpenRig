//! Chain input/output tooltip helpers — lifted out of project_view.rs
//! so the parent module stays under the size cap.

use infra_cpal::AudioDeviceDescriptor;
use project::chain::{Chain, ChainInputMode, ChainOutputMode};
use project::project::Project;

use crate::project_view::format_channel_list;

pub(crate) fn chain_inputs_tooltip(
    chain: &Chain,
    _project: &Project,
    devices: &[AudioDeviceDescriptor],
) -> String {
    // Show only entries from the FIRST InputBlock (chip In)
    let first_input = chain.first_input();
    let Some(input) = first_input else {
        return "No input configured".to_string();
    };
    input.entries.iter().enumerate().map(|(ei, entry)| {
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
            format!("{}: {} · {} · Ch {}", label, device_name, mode, format_channel_list(&entry.channels))
    }).collect::<Vec<_>>().join("\n")
}

pub(crate) fn chain_outputs_tooltip(
    chain: &Chain,
    _project: &Project,
    devices: &[AudioDeviceDescriptor],
) -> String {
    // Show only entries from the LAST OutputBlock (chip Out)
    let last_output = chain.last_output();
    let Some(output) = last_output else {
        return "No output configured".to_string();
    };
    output.entries.iter().enumerate().map(|(ei, entry)| {
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
        format!("{}: {} · {} · Ch {}", label, device_name, mode, format_channel_list(&entry.channels))
    }).collect::<Vec<_>>().join("\n")
}
