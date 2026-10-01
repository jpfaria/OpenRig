//! Shared fixtures for the #328 split command tests.
//!
//! One chain (`CHAIN`) whose blocks each test sets. Later tasks add a
//! one-input rig for the tests that follow an edit into `project.openrig`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use domain::ids::BlockId;
use project::block::{AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::endpoint_disables::EndpointDisables;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};

use crate::command_schema::command_from_variant;
use crate::dispatcher::CommandDispatcher;
use crate::event::Event;
use crate::local_dispatcher::LocalDispatcher;
use crate::local_dispatcher_tests::{make_core_block, make_project};

/// The chain every non-rig split test edits.
pub(crate) const CHAIN: &str = "chain_0";

/// A split block carrying the default knobs of spec §1.2.
pub(crate) fn split(id: &str, end: SplitEnd, a: Vec<AudioBlock>, b: Vec<AudioBlock>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Split(SplitBlock::with_paths(end, vec![a, b])),
    }
}

/// `[pre, split_0 → Mix { a: [a_0], b: [b_0] }, post]`.
pub(crate) fn mix_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Mix,
            vec![make_core_block("a_0", true)],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("post", true),
    ]
}

/// A project whose only chain, `CHAIN`, holds exactly `blocks`.
pub(crate) fn project_with(blocks: Vec<AudioBlock>) -> Rc<RefCell<Project>> {
    let project = make_project(CHAIN, make_core_block("seed", true));
    project.borrow_mut().chains[0].blocks = blocks;
    project
}

/// The split of the project's first chain (panics when it has none).
pub(crate) fn split_of(project: &Project) -> SplitBlock {
    project.chains[0]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) => Some(split.clone()),
            _ => None,
        })
        .expect("the first chain holds a split")
}

/// The ids of `blocks`, in order.
pub(crate) fn ids(blocks: &[AudioBlock]) -> Vec<String> {
    blocks.iter().map(|b| b.id.0.clone()).collect()
}

/// `[pre, split_0 → Y { a: [a_0], b: [b_0] }]` — the split is last, as Y requires.
pub(crate) fn y_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "split_0",
            SplitEnd::Y,
            vec![make_core_block("a_0", true)],
            vec![make_core_block("b_0", true)],
        ),
    ]
}

/// `[pre, mix → Mix { a: [a_0], b: [b_0] }, mid, y → Y { a: [ya_0], b: [yb_0] }]`
/// — the Mix first and the Y last, as spec §1.1 allows.
pub(crate) fn mix_then_y_chain() -> Vec<AudioBlock> {
    vec![
        make_core_block("pre", true),
        split(
            "mix",
            SplitEnd::Mix,
            vec![make_core_block("a_0", true)],
            vec![make_core_block("b_0", true)],
        ),
        make_core_block("mid", true),
        split(
            "y",
            SplitEnd::Y,
            vec![make_core_block("ya_0", true)],
            vec![make_core_block("yb_0", true)],
        ),
    ]
}

/// The split `id` of the project's first chain (panics when it is missing).
pub(crate) fn split_by_id(project: &Project, id: &str) -> SplitBlock {
    project.chains[0]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) if b.id.0 == id => Some(split.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("the first chain holds the split '{id}'"))
}

/// Dispatch a command given as its wire form — the exact road an MCP tool
/// call or a `midi-map.yaml` line takes (`command_from_variant`).
pub(crate) fn dispatch_json(
    dispatcher: &LocalDispatcher,
    variant: &str,
    args: serde_json::Value,
) -> anyhow::Result<Vec<Event>> {
    dispatcher.dispatch(command_from_variant(variant, args)?)
}

/// The projected chain of the rig built by [`rig_with_presets`].
pub(crate) const RIG_CHAIN: &str = "rig:in";

/// A one-input rig (`in`, bound to the `io-main` E/S, so no I/O blocks are
/// synthesized) whose bank holds `presets` at positions 1, 2, … in order.
pub(crate) fn rig_with_presets(presets: Vec<(&str, Vec<AudioBlock>)>) -> RigProject {
    let mut bank = BTreeMap::new();
    let mut pool = BTreeMap::new();
    for (slot, (name, blocks)) in presets.into_iter().enumerate() {
        bank.insert(slot + 1, name.to_string());
        pool.insert(
            name.to_string(),
            RigPreset::from_legacy_blocks(blocks, 100.0),
        );
    }
    let input = RigInput {
        mix: Default::default(),
        label: None,
        bank,
        active_preset: 1,
        active_scene: 1,
        routing: Vec::new(),
        instrument: "electric_guitar".to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: vec!["io-main".to_string()],
        loopers: Vec::new(),
        disabled_endpoints: EndpointDisables::default(),
    };
    RigProject {
        name: None,
        inputs: BTreeMap::from([("in".to_string(), input)]),
        outputs: BTreeMap::new(),
        presets: pool,
        midi: None,
        chain_order: Vec::new(),
    }
}

/// The session a rig project opens into: the rig, its projected chains, and a
/// dispatcher attached to both.
pub(crate) fn rig_session_from(
    rig: RigProject,
) -> (
    Rc<RefCell<RigProject>>,
    Rc<RefCell<Project>>,
    LocalDispatcher,
) {
    let rig = Rc::new(RefCell::new(rig));
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig.borrow(),
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::clone(&rig));
    (rig, project, dispatcher)
}

/// The split stored in the rig preset `name` (panics when it has none).
pub(crate) fn preset_split(rig: &RigProject, name: &str) -> SplitBlock {
    rig.presets[name]
        .blocks
        .iter()
        .find_map(|b| match &b.kind {
            AudioBlockKind::Split(split) => Some(split.clone()),
            _ => None,
        })
        .expect("the preset holds a split")
}
