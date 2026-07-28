//! #716 RED (#1, UI-projection angle) — after reopening, the editor checklist
//! must show the previously-selected binding as CHECKED.
//!
//! This drives the exact projection the chain editor uses to build the
//! checklist (`binding_choices(registry, chain.io_binding_ids)`) against the
//! reopened chain. Because the rig drops `io_binding_ids` on reopen, the
//! projection marks every row unchecked — what the user sees.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use adapter_gui::chain_binding_choices::binding_choices;
use application::command::{ChainCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;
use domain::ids::{ChainId, DeviceId};
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::rig::{RigInput, RigPreset, RigProject};

fn rig_with_chain() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(Vec::new(), 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: None,
            // #716: the input starts unbound; the test binds it via
            // SetChainIoBindings below and asserts the checklist reflects it
            // after reopen. Device data lives in the registry, not in the input.
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

fn registry() -> Vec<IoBinding> {
    vec![IoBinding {
        id: "main".into(),
        name: "Main".into(),
        inputs: vec![IoEndpoint {
            name: "Guitar In".into(),
            device_id: DeviceId("hw:0,0".into()),
            mode: ChannelMode::Mono,
            channels: vec![0],
        }],
        outputs: vec![],
    }]
}

#[test]
fn reopened_checklist_shows_selected_binding_checked() {
    let rig = Rc::new(RefCell::new(rig_with_chain()));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));

    dispatcher
        .dispatch(Command::Chain(ChainCommand::SetChainIoBindings {
            chain: ChainId("rig:in".to_string()),
            binding_ids: vec!["main".to_string()],
        }))
        .expect("SetChainIoBindings must succeed");

    // Reopen + build the editor checklist exactly like chain_crud_wiring does.
    let reopened = engine::rig_runtime::rig_to_legacy_project(&rig.borrow(), &BTreeSet::new());
    let chain = reopened
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:in")
        .expect("chain must exist after reopen");
    let choices = binding_choices(&registry(), &chain.io_binding_ids);

    let main = choices
        .iter()
        .find(|c| c.id.as_str() == "main")
        .expect("binding row present");
    assert!(
        main.selected,
        "the editor checklist must show binding 'main' CHECKED after reopen; it is unchecked \
         because the reopened chain's io_binding_ids = {:?}",
        chain.io_binding_ids
    );
}
