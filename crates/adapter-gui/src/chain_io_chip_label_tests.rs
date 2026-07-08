// Tests for the chain I/O chip label projector (#716).
//
// The chip label shown on the chain row's IN/OUT endpoint chips must display
// the human-readable binding name (e.g. "Scarlett") instead of the raw device
// id string. A pure projector derives the label from the chain's head input /
// tail output binding reference and the app config's io_bindings list.
//
// Test contract:
//   - `chain_io_chip_label_shows_binding_name`: chain bound to "main" →
//     displays the binding's `name` field, not the device id.
//   - `chain_io_chip_label_unbound`: chain with no I/O blocks → returns "".
//   - `compact_configure_io_routes_to_picker`: source-presence test — the
//     compact chain callbacks wire `on_configure_input` / `on_configure_output`
//     to the main window's `invoke_configure_chain_input/output`, which is the
//     same binding-picker path as the chain editor.

use crate::ui_state::chain_io_chip_label;
use domain::ids::{BlockId, ChainId};
use infra_filesystem::{AppConfig, IoBinding};
use project::block::{AudioBlock, AudioBlockKind, InputBlock};
use project::chain::Chain;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn config_with_binding(id: &str, name: &str) -> AppConfig {
    AppConfig {
        io_bindings: vec![IoBinding {
            id: id.to_string(),
            name: name.to_string(),
            inputs: vec![],
            outputs: vec![],
        }],
        ..Default::default()
    }
}

fn chain_with_input_binding(binding_id: &str, endpoint: &str) -> Chain {
    Chain {
        id: ChainId("test:chain".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![AudioBlock {
            id: BlockId("test:input".to_string()),
            enabled: true,
            kind: AudioBlockKind::Input(InputBlock {
                model: "standard".to_string(),
                io: binding_id.to_string(),
                endpoint: endpoint.to_string(),
            }),
        }],
        di_output: None,
    }
}

fn chain_empty() -> Chain {
    Chain {
        id: ChainId("test:empty".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// A chain whose head input references binding "main" (name "Scarlett") →
/// the chip projector returns the binding name, not the device id.
#[test]
fn chain_io_chip_label_shows_binding_name() {
    let config = config_with_binding("main", "Scarlett");
    let chain = chain_with_input_binding("main", "Guitar In");
    let label = chain_io_chip_label(&chain, &config, true);
    assert_eq!(
        label, "Scarlett",
        "chip label must be the binding name, not the device id"
    );
}

/// A chain with no I/O blocks → returns empty string.
/// Callers treat "" as "unbound" and render the raw chip icon without a label.
#[test]
fn chain_io_chip_label_unbound() {
    let config = AppConfig::default();
    let chain = chain_empty();
    let label = chain_io_chip_label(&chain, &config, true);
    assert_eq!(
        label, "",
        "unbound chain (no input blocks) must return empty string"
    );
}

/// Source-presence test: the compact chain callbacks module must wire
/// `on_configure_input` / `on_configure_output` to the main window's binding
/// picker entry points (`invoke_configure_chain_input/output`), ensuring the
/// compact view's "configure I/O" button opens the same picker as the chain
/// editor — not a raw-device flow.
#[test]
fn compact_configure_io_routes_to_picker() {
    use std::path::PathBuf;

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/compact_chain_callbacks.rs");
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    assert!(
        src.contains("on_configure_input"),
        "compact_chain_callbacks.rs must register on_configure_input"
    );
    assert!(
        src.contains("on_configure_output"),
        "compact_chain_callbacks.rs must register on_configure_output"
    );
    assert!(
        src.contains("invoke_configure_chain_input"),
        "compact on_configure_input must delegate to invoke_configure_chain_input \
         (the binding-picker entry point) — not a raw-device flow"
    );
    assert!(
        src.contains("invoke_configure_chain_output"),
        "compact on_configure_output must delegate to invoke_configure_chain_output \
         (the binding-picker entry point) — not a raw-device flow"
    );
}
