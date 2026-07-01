//! Issue #537 — red-first fixture test for the cab→preamp regression.
//!
//! User reproduction (live session, 2026-05-25):
//!   1. Project with a chain containing a `cab/ir_marshall_4x12_v30` block.
//!   2. Open the model picker on that slot (effect_type='cab').
//!   3. Select another cab IR from the picker (`ir_v30_4x12`).
//!   4. Slot is rebuilt as `preamp/ir_v30_4x12` — the engine then routes
//!      the IR convolver through a preamp slot and audio becomes white
//!      noise ("radio interference").
//!
//! The contract: `Command::ReplaceBlockModel` must derive the new
//! `effect_type` from the new model's manifest `type:` field. Both
//! marshall_4x12_v30 and v30_4x12 declare `type: cab` in their
//! `manifest.yaml`, so after the swap the slot must still be `cab`.
//!
//! The dispatcher uses the global `plugin_loader::registry`, so this
//! test initializes it from the user's `OpenRig-plugins/plugins/source`
//! tree (same pattern as `crates/project/tests/disk_package_metadata_lookups.rs`).
//! If neither candidate root resolves, the test fails loudly — the
//! repro depends on the disk packages being present.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Once;

use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;

use application::command::Command;
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

fn init_plugins() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let candidates = [
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../../OpenRig-plugins/plugins/source"),
            PathBuf::from(
                "/Users/joao.faria/Projetos/github.com/jpfaria/OpenRig-plugins/plugins/source",
            ),
        ];
        let roots: Vec<PathBuf> = candidates.into_iter().filter(|p| p.is_dir()).collect();
        assert!(
            !roots.is_empty(),
            "issue #537 repro requires OpenRig-plugins/plugins/source to be \
             present on disk — none of the candidate roots resolved"
        );
        plugin_loader::registry::init_many(&roots);
    });
}

fn make_project_with_cab(model_id: &str) -> Rc<RefCell<Project>> {
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
fn issue_537_both_disk_packages_register_as_cab() {
    init_plugins();
    let models = project::catalog::supported_block_models("cab")
        .expect("cab catalog must exist after registry init");
    let ids: Vec<&str> = models.iter().map(|m| m.model_id.as_str()).collect();
    assert!(
        ids.contains(&"ir_marshall_4x12_v30"),
        "ir_marshall_4x12_v30 must be registered as a cab model — got cab ids: {:?}",
        ids
    );
    assert!(
        ids.contains(&"ir_v30_4x12"),
        "ir_v30_4x12 must be registered as a cab model (its manifest declares `type: cab`) — \
         got cab ids: {:?}",
        ids
    );
}

#[test]
fn issue_537_replace_marshall_with_v30_keeps_cab_effect_type() {
    init_plugins();
    let project = make_project_with_cab("ir_marshall_4x12_v30");
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));

    let result = dispatcher.dispatch(Command::ReplaceBlockModel {
        chain: ChainId("chain_0".to_string()),
        block: BlockId("blk_cab".to_string()),
        model_id: "ir_v30_4x12".to_string(),
    });

    assert!(
        result.is_ok(),
        "dispatch returned Err while swapping one cab IR for another: {:?}",
        result
    );
    let proj = project.borrow();
    let block = &proj.chains[0].blocks[0];
    let core = match &block.kind {
        AudioBlockKind::Core(cb) => cb,
        other => panic!(
            "expected Core variant after cab IR swap, got '{}' (slot must stay a Core/cab block)",
            other.label()
        ),
    };
    assert_eq!(
        core.effect_type, "cab",
        "REGRESSION: effect_type morphed to '{}' after swapping two cab IRs — slot must stay \
         'cab' so the engine keeps routing through the cab convolution path (model now: '{}')",
        core.effect_type, core.model
    );
    assert_eq!(
        core.model, "ir_v30_4x12",
        "model must be the newly picked 'ir_v30_4x12'"
    );
}
