//! Responsibility: shapes the endpoint bindings for the screen to bind to.

use infra_filesystem::{AppConfig, ChannelMode, IoBinding, IoEndpoint};

use crate::chain_io_labels::channels_label;

/// Rust-side mirror of the Slint `IoEndpointModel` struct.
///
/// All fields carry display-ready strings so Slint components need no
/// further formatting. `device_label` is the raw `DeviceId` string;
/// `channels_label` is 1-based (e.g. `"1, 2"`); `mode` is the
/// snake_case wire token (`"mono"`, `"stereo"`, `"dual_mono"`).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoEndpointModel {
    pub name: String,
    pub device_label: String,
    pub mode: String,
    pub channels_label: String,
}

/// Rust-side mirror of the Slint `IoBindingModel` struct.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoBindingModel {
    pub id: String,
    pub name: String,
    pub inputs: Vec<IoEndpointModel>,
    pub outputs: Vec<IoEndpointModel>,
}

#[allow(dead_code)]
fn channel_mode_label(mode: ChannelMode) -> &'static str {
    match mode {
        ChannelMode::Mono => "mono",
        ChannelMode::Stereo => "stereo",
        ChannelMode::DualMono => "dual_mono",
    }
}

#[allow(dead_code)]
fn endpoint_model(ep: &IoEndpoint) -> IoEndpointModel {
    IoEndpointModel {
        name: ep.name.clone(),
        device_label: ep.device_id.0.clone(),
        mode: channel_mode_label(ep.mode).to_string(),
        channels_label: channels_label(&ep.channels),
    }
}

/// Projects `config.io_bindings` into display-ready `IoBindingModel` values.
///
/// Pure function — safe to call in tests without `AppWindow`.
#[allow(dead_code)]
pub fn ui_bindings(config: &AppConfig) -> Vec<IoBindingModel> {
    config
        .io_bindings
        .iter()
        .map(|b: &IoBinding| IoBindingModel {
            id: b.id.clone(),
            name: b.name.clone(),
            inputs: b.inputs.iter().map(endpoint_model).collect(),
            outputs: b.outputs.iter().map(endpoint_model).collect(),
        })
        .collect()
}

/// Given a block's `(io, endpoint)` string pair, looks up the matching
/// `IoEndpointModel` from `config.io_bindings`.
///
/// Returns `None` when `io` is empty (unbound block), or when the binding
/// or endpoint name is not found.
///
/// Searches both `inputs` and `outputs` of the matched binding so callers
/// don't need to know which side the endpoint lives on.
///
/// Pure function — safe to call in tests without `AppWindow`.
#[allow(dead_code)]
pub fn resolve_block_io_endpoint(
    config: &AppConfig,
    io: &str,
    endpoint: &str,
) -> Option<IoEndpointModel> {
    if io.is_empty() {
        return None;
    }
    let binding = config.io_bindings.iter().find(|b| b.id == io)?;
    binding
        .inputs
        .iter()
        .chain(binding.outputs.iter())
        .find(|ep| ep.name == endpoint)
        .map(endpoint_model)
}
