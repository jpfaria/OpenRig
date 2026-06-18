use infra_filesystem::{AppConfig, ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockDrawerMode {
    Add,
    Edit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockDrawerState {
    pub mode: BlockDrawerMode,
    pub title: &'static str,
    pub confirm_label: &'static str,
    pub effect_type: String,
    pub model_id: Option<String>,
}

pub fn block_drawer_state(
    block_index: Option<usize>,
    effect_type: &str,
    model_id: Option<&str>,
) -> BlockDrawerState {
    let mode = if block_index.is_some() {
        BlockDrawerMode::Edit
    } else {
        BlockDrawerMode::Add
    };

    BlockDrawerState {
        mode,
        title: "",
        confirm_label: if matches!(mode, BlockDrawerMode::Edit) {
            "Salvar"
        } else {
            "Adicionar"
        },
        effect_type: effect_type.to_string(),
        model_id: model_id.map(ToString::to_string),
    }
}

/// Returns the accent color (RGBA) for an effect type icon_kind.
/// Single source of truth — used by all UI components.
pub fn accent_color_for_icon_kind(icon_kind: &str) -> slint::Color {
    match icon_kind {
        "preamp" => slint::Color::from_argb_u8(255, 0xf2, 0x9f, 0x38),
        "amp" => slint::Color::from_argb_u8(255, 0xf0, 0x62, 0x92),
        "cab" => slint::Color::from_argb_u8(255, 0xf2, 0xcb, 0x54),
        "body" => slint::Color::from_argb_u8(255, 0xc8, 0x94, 0x6a),
        "ir" => slint::Color::from_argb_u8(255, 0x7c, 0xc9, 0xff),
        "full_rig" => slint::Color::from_argb_u8(255, 0x63, 0xd2, 0xff),
        "gain" => slint::Color::from_argb_u8(255, 0xff, 0x6a, 0x57),
        "dynamics" => slint::Color::from_argb_u8(255, 0x41, 0xb8, 0xff),
        "filter" => slint::Color::from_argb_u8(255, 0xd6, 0xc8, 0x5a),
        "wah" => slint::Color::from_argb_u8(255, 0x78, 0xd0, 0x6b),
        "modulation" => slint::Color::from_argb_u8(255, 0x58, 0xd3, 0x9b),
        "delay" => slint::Color::from_argb_u8(255, 0xba, 0x8c, 0xff),
        "reverb" => slint::Color::from_argb_u8(255, 0x6d, 0xe1, 0xd2),
        "utility" => slint::Color::from_argb_u8(255, 0x95, 0xa0, 0xb2),
        "nam" => slint::Color::from_argb_u8(255, 0xff, 0x7c, 0xd7),
        "pitch" => slint::Color::from_argb_u8(255, 0x8f, 0x8c, 0xff),
        "insert" => slint::Color::from_argb_u8(255, 0xf2, 0x9f, 0x38),
        "input" => slint::Color::from_argb_u8(255, 0x45, 0xa7, 0xff),
        "output" => slint::Color::from_argb_u8(255, 0x45, 0xa7, 0xff),
        _ => slint::Color::from_argb_u8(255, 0x7f, 0xb0, 0xff),
    }
}

/// Icon SVG index for an icon_kind. Used to load the correct icon from a pre-built array.
/// Returns a numeric index into EFFECT_TYPE_ICONS.
#[allow(dead_code)]
pub fn icon_index_for_icon_kind(icon_kind: &str) -> usize {
    match icon_kind {
        "preamp" => 0,
        "amp" => 1,
        "cab" => 2,
        "body" => 3,
        "ir" => 4,
        "full_rig" => 5,
        "gain" => 6,
        "dynamics" => 7,
        "filter" => 8,
        "wah" => 9,
        "modulation" => 10,
        "delay" => 11,
        "reverb" => 12,
        "utility" => 13,
        "nam" => 14,
        "pitch" => 15,
        _ => 13, // utility fallback
    }
}

pub fn block_family_for_kind(kind: &str) -> &'static str {
    use block_core::*;
    match kind {
        EFFECT_TYPE_PREAMP | EFFECT_TYPE_AMP | EFFECT_TYPE_FULL_RIG | EFFECT_TYPE_NAM => "amp",
        EFFECT_TYPE_CAB => "cab",
        EFFECT_TYPE_BODY => "body",
        EFFECT_TYPE_IR => "ir",
        EFFECT_TYPE_GAIN => "gain",
        EFFECT_TYPE_DYNAMICS => "dynamics",
        EFFECT_TYPE_FILTER => "filter",
        EFFECT_TYPE_WAH => "wah",
        EFFECT_TYPE_PITCH => "pitch",
        EFFECT_TYPE_MODULATION => "modulation",
        EFFECT_TYPE_DELAY | EFFECT_TYPE_REVERB => "space",
        EFFECT_TYPE_UTILITY => "utility",
        "input" | "output" | "insert" => "routing",
        _ => "utility",
    }
}

// ── I/O binding Slint bridge (#716) ──────────────────────────────────────────

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

#[cfg(test)]
pub fn insertion_slot_indices(block_count: usize) -> Vec<usize> {
    (0..=block_count).collect()
}

pub fn chain_routing_summary(chain: &Chain) -> String {
    let input_channels: Vec<usize> = chain
        .input_blocks()
        .into_iter()
        .flat_map(|(_, ib)| ib.entries.iter().flat_map(|e| e.channels.iter().copied()))
        .collect();
    let output_channels: Vec<usize> = chain
        .output_blocks()
        .into_iter()
        .flat_map(|(_, ob)| ob.entries.iter().flat_map(|e| e.channels.iter().copied()))
        .collect();
    format!(
        "Entrada {} -> Saida {}",
        channels_label(&input_channels),
        channels_label(&output_channels),
    )
}

fn channels_label(channels: &[usize]) -> String {
    channels
        .iter()
        .map(|channel| (channel + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns the input endpoint names for the given binding.
///
/// Used by the endpoint picker to populate its list when the user selects
/// a binding in the I/O picker (input block context).
///
/// Pure function — no side effects.
pub(crate) fn endpoint_names_for_input_binding(binding: &IoBindingModel) -> Vec<String> {
    binding.inputs.iter().map(|ep| ep.name.clone()).collect()
}

/// Returns the output endpoint names for the given binding.
///
/// Used by the endpoint picker to populate its list when the user selects
/// a binding in the I/O picker (output block context).
///
/// Pure function — no side effects.
pub(crate) fn endpoint_names_for_output_binding(binding: &IoBindingModel) -> Vec<String> {
    binding.outputs.iter().map(|ep| ep.name.clone()).collect()
}

#[cfg(test)]
#[path = "ui_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "endpoint_picker_tests.rs"]
mod endpoint_picker_tests;
