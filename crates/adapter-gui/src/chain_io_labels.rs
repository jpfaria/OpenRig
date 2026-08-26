//! Responsibility: writes the label a chain shows for its endpoints.

use infra_filesystem::{AppConfig, IoBinding};
use project::chain::Chain;

pub fn chain_routing_summary(chain: &Chain, io_bindings: &[IoBinding]) -> String {
    // #716: device endpoints resolve from the binding registry, not from
    // block `entries`.
    let (resolved_inputs, resolved_outputs) =
        engine::runtime_endpoints::resolve_chain_io(chain, io_bindings);
    let input_channels: Vec<usize> = resolved_inputs
        .iter()
        .flat_map(|e| e.channels.iter().copied())
        .collect();
    let output_channels: Vec<usize> = resolved_outputs
        .iter()
        .flat_map(|e| e.channels.iter().copied())
        .collect();
    format!(
        "Entrada {} -> Saida {}",
        channels_label(&input_channels),
        channels_label(&output_channels),
    )
}

pub(crate) fn channels_label(channels: &[usize]) -> String {
    channels
        .iter()
        .map(|channel| (channel + 1).to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns the display label for the chain's head input or tail output chip.
///
/// Looks up the binding name from `io_bindings` using the `io` field of the
/// chain's first Input block (for `is_input = true`) or last Output block
/// (for `is_input = false`). Returns the binding's human-readable `name` field
/// (e.g. `"Scarlett"`) so the chip shows a meaningful label instead of a raw
/// device id string.
///
/// Returns `""` when:
/// - The chain has no input/output block (`io` is unset), or
/// - The `io` field is empty (unbound block), or
/// - The binding id is not found in `io_bindings`.
///
/// Pure function — safe to call in tests without `AppWindow`.
#[allow(dead_code)]
pub fn chain_io_chip_label(chain: &Chain, config: &AppConfig, is_input: bool) -> String {
    chain_io_chip_label_from_bindings(chain, &config.io_bindings, is_input)
}

/// Inner variant that takes the binding slice directly — used by
/// `replace_project_chains` which has `&[IoBinding]` but not a full
/// `AppConfig`.
pub(crate) fn chain_io_chip_label_from_bindings(
    chain: &Chain,
    io_bindings: &[IoBinding],
    is_input: bool,
) -> String {
    let io_ref = if is_input {
        chain.first_input().map(|ib| ib.io.as_str())
    } else {
        chain.last_output().map(|ob| ob.io.as_str())
    };
    let io = match io_ref {
        Some(s) if !s.is_empty() => s,
        _ => return String::new(),
    };
    io_bindings
        .iter()
        .find(|b| b.id == io)
        .map(|b| b.name.clone())
        .unwrap_or_default()
}
