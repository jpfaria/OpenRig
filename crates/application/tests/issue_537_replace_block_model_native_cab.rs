//! Issue #537 — positive contract: swapping one **native** cab model for
//! another keeps the slot's `effect_type` as `"cab"`.
//!
//! The disk-package side of #537 lives in
//! `issue_537_replace_block_model_disk_package_cab.rs` (sibling). This
//! file pins the parallel native code path so the fix in
//! `resolve_effect_type_for_model` cannot accidentally damage native
//! swaps while routing disk packages through the registry's manifest
//! type.

use std::cell::RefCell;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

fn make_project_with_native_cab(model_id: &str) -> Rc<RefCell<Project>> {
    Rc::new(RefCell::new(Project {
        name: None,
        device_settings: vec![],
        midi: None,
        chains: vec![Chain {
            id: ChainId("chain_0".to_string()),
            description: None,
            instrument: "electric_guitar".to_string(),
            enabled: true,
            volume: 100.0,
            io_binding_ids: vec![],
            blocks: vec![AudioBlock {
                id: BlockId("blk_cab".to_string()),
                enabled: true,
                kind: AudioBlockKind::Core(CoreBlock {
                    effect_type: "cab".to_string(),
                    model: model_id.to_string(),
                    params: ParameterSet::default(),
                }),
            }],
            di_output: None,
        }],
    }))
}

#[test]
fn issue_537_native_cab_swap_keeps_cab_effect_type() {
    let project = make_project_with_native_cab("american_2x12");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_cab".to_string()),
        model_id: "brit_4x12".to_string(),
    });

    assert!(result.is_ok(), "dispatch returned Err: {:?}", result);
    let proj = project.borrow();
    let block = &proj.chains[0].blocks[0];
    let core = match &block.kind {
        AudioBlockKind::Core(cb) => cb,
        other => panic!(
            "expected Core variant after native cab swap, got '{}'",
            other.label()
        ),
    };
    assert_eq!(
        core.effect_type, "cab",
        "effect_type must stay 'cab' after swapping one native cab for another \
         (got '{}' on slot now hosting '{}')",
        core.effect_type, core.model
    );
    assert_eq!(
        core.model, "brit_4x12",
        "model must be the newly picked 'brit_4x12'"
    );
}
