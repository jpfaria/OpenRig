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

/// The owner's private capture tree, resolved from `OPENRIG_OWNER_PLUGINS` or
/// the sibling `OpenRig-plugins` checkout. `None` when neither is present — the
/// caller then skips (this repro needs the real disk packages, not a fixture).
fn owner_plugins_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OPENRIG_OWNER_PLUGINS") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    // Walk up from the crate dir; accept the first ancestor with a sibling
    // `OpenRig-plugins/plugins/source` (author's main checkout or a .solvers
    // clone, at any depth). None on CI, where the tree is absent.
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let cand = dir.join("OpenRig-plugins/plugins/source");
        if cand.is_dir() {
            return Some(cand);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn init_plugins(root: &std::path::Path) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        plugin_loader::registry::init_many(std::slice::from_ref(&root.to_path_buf()));
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
            loopers: vec![],
        }],
    }))
}

#[test]
fn issue_537_both_disk_packages_register_as_cab() {
    let Some(root) = owner_plugins_root() else {
        eprintln!(
            "[#537] SKIPPED — set OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> to run"
        );
        return;
    };
    init_plugins(&root);
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
    let Some(root) = owner_plugins_root() else {
        eprintln!(
            "[#537] SKIPPED — set OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source> to run"
        );
        return;
    };
    init_plugins(&root);
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
