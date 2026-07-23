// Per-stream meter row count on the chain card (#750).
//
// The footer of a chain card renders one per-stream meter row per element of
// `ProjectChainItem.stream_meters` (`chain_row.slint`). Two bugs motivated
// these tests:
//
//   - On project open the row count was wrong: every open path handed
//     `replace_project_chains` an EMPTY io_bindings slice, so the resolved
//     input count was 0 and the `.max(1)` clamp showed a single phantom row
//     even for a chain that binds four inputs.
//   - The per-stream graph stayed on screen while the chain was disabled. The
//     graph is a live surface — it must not show at all unless the chain is
//     enabled.
//
// Contract: `stream_meters.len()` == 0 when the chain is disabled, and one row
// per resolved input endpoint when it is enabled.

use crate::project_view::replace_project_chains;
use crate::ProjectChainItem;
use domain::ids::{ChainId, DeviceId};
use infra_filesystem::{ChannelMode, IoBinding, IoEndpoint};
use project::chain::Chain;
use project::project::Project;
use slint::{Model, VecModel};
use std::rc::Rc;

// ── Helpers ────────────────────────────────────────────────────────────────

/// A binding whose `inputs` holds `count` independent mono endpoints — the
/// "two devices, two channels each → four streams" shape the bug reproduces.
fn binding_with_inputs(id: &str, count: usize) -> IoBinding {
    IoBinding {
        id: id.to_string(),
        name: id.to_string(),
        inputs: (0..count)
            .map(|i| IoEndpoint {
                name: format!("In {}", i + 1),
                device_id: DeviceId(format!("dev:{}", i / 2)),
                mode: ChannelMode::Mono,
                channels: vec![i % 2],
            })
            .collect(),
        outputs: vec![],
    }
}

/// A chain that binds its head input to `binding_id` and carries no blocks, so
/// its resolved input count is exactly the binding's input endpoints.
fn chain_bound_to(binding_id: &str, enabled: bool) -> Chain {
    Chain {
        id: ChainId("test:chain".to_string()),
        description: None,
        instrument: "electric_guitar".to_string(),
        enabled,
        volume: 100.0,
        io_binding_ids: vec![binding_id.to_string()],
        blocks: vec![],
        di_output: None,
    }
}

fn project_with(chain: Chain) -> Project {
    Project {
        name: None,
        device_settings: Vec::new(),
        chains: vec![chain],
        midi: None,
    }
}

fn stream_meter_count(project: &Project, bindings: &[IoBinding]) -> usize {
    let model = Rc::new(VecModel::<ProjectChainItem>::default());
    replace_project_chains(&model, project, &[], &[], bindings);
    model
        .row_data(0)
        .expect("one chain row")
        .stream_meters
        .row_count()
}

// ── Tests ──────────────────────────────────────────────────────────────────

/// A disabled chain shows NO per-stream meter rows — the live graph is hidden
/// until the chain is enabled.
#[test]
fn disabled_chain_shows_no_stream_meter_rows() {
    let bindings = vec![binding_with_inputs("main", 4)];
    let project = project_with(chain_bound_to("main", false));
    assert_eq!(
        stream_meter_count(&project, &bindings),
        0,
        "disabled chain must render zero per-stream meter rows"
    );
}

/// An enabled chain shows one per-stream meter row per resolved input endpoint
/// — four bound inputs → four rows (not the `.max(1)` phantom single row).
#[test]
fn enabled_chain_shows_one_row_per_resolved_input() {
    let bindings = vec![binding_with_inputs("main", 4)];
    let project = project_with(chain_bound_to("main", true));
    assert_eq!(
        stream_meter_count(&project, &bindings),
        4,
        "enabled chain must render one meter row per resolved input endpoint"
    );
}
