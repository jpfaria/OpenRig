//! #716 RED (#6/#7) — activating a checklist-bound chain must actually start it.
//!
//! User repro: open project TESTE, activate chain 1 → NO SOUND; toggling a
//! block logs `sync_block_toggle: fast path declined (runtime not started)`.
//!
//! Root: cold activation (`schedule_chain_activation`) computed the chain's
//! input device set from `input_blocks().entries`, which is EMPTY for a
//! binding-bound chain (its I/O is `io_binding_ids`, no device blocks). With 0
//! devices it returned `Ok(false)` → the chain never activates → no runtime →
//! no sound, and block toggles find "runtime not started".
//!
//! These drive the real cold-activate seam with the checklist (io_binding_ids)
//! chain.

use std::collections::HashMap;

use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use engine::runtime::RuntimeGraph;
use infra_cpal::ProjectRuntimeController;
use project::chain::Chain;
use project::project::Project;

fn one_binding() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "io".into(),
        name: "Interface".into(),
        inputs: vec![IoEndpoint {
            name: "in".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![IoEndpoint {
            name: "out".into(),
            device_id: DeviceId("dev".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
    }]
}

/// The chain as the #716 editor checklist saves it: bound via `io_binding_ids`,
/// no per-block device Input/Output blocks.
fn checklist_bound_chain() -> Chain {
    Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["io".into()],
        blocks: vec![],
    }
}

fn project_with(chain: &Chain) -> Project {
    Project {
        name: None,
        device_settings: vec![],
        chains: vec![chain.clone()],
        midi: None,
    }
}

#[test]
fn checklist_bound_chain_schedules_cold_activation() {
    let chain = checklist_bound_chain();
    let project = project_with(&chain);
    let mut controller =
        ProjectRuntimeController::for_testing(RuntimeGraph { chains: HashMap::new() });
    controller.set_io_bindings(one_binding());

    let scheduled = controller
        .schedule_chain_activation(&project, &chain)
        .expect("activation query must not error");

    assert!(
        scheduled,
        "a single-device checklist-bound chain (io_binding_ids) must cold-activate \
         (schedule its off-thread runtime build); got false → it never starts → no sound \
         and block toggles report 'runtime not started'"
    );
}

#[test]
fn activating_a_checklist_bound_chain_makes_the_controller_running() {
    let chain = checklist_bound_chain();
    let project = project_with(&chain);
    let mut controller =
        ProjectRuntimeController::for_testing(RuntimeGraph { chains: HashMap::new() });
    controller.set_io_bindings(one_binding());

    let _ = controller.schedule_chain_activation(&project, &chain);

    assert!(
        controller.is_running(),
        "after activating the bound chain the controller must be running (the chain is live); \
         it is not → the chain never started → no sound"
    );
}
