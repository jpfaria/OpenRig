//! #328 — #986 inside a split path: changing the model of the amp that sits
//! in path B, while scene 2 is active, must leave the LIVE amp sounding like
//! scene 2 resolves it (its surviving `input`/`output` overrides applied), not
//! like the new model's bare defaults.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::rc::Rc;

use domain::ids::{BlockId, ChainId};
use project::block::{find_block_mut, AudioBlock, AudioBlockKind, SplitBlock, SplitEnd};
use project::rig::RigProject;

use application::command::{BlockCommand, Command};
use application::dispatcher::CommandDispatcher;
use application::local_dispatcher::LocalDispatcher;

const INPUT: &str = "input-7";
const CHAIN: &str = "rig:input-7";
const PRESET: &str = "anal-dig";
const AMP: &str = "rig:input-7:block:amp";
const NEW_MODEL: &str = "chime";

/// The #986 fixture with its amp moved into path B of a Split → Mix, scene 2
/// (the scene that overrides the amp) active.
fn rig_with_the_amp_in_path_b() -> RigProject {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/issue_986_scened_preset.yaml");
    let mut rig = infra_yaml::load_rig_project_file(&path).expect("load the #986 fixture rig");
    let preset = rig.presets.get_mut(PRESET).expect("fixture preset");
    let at = preset
        .blocks
        .iter()
        .position(|b| b.id.0 == AMP)
        .expect("the fixture amp");
    let amp = preset.blocks.remove(at);
    preset.blocks.insert(
        at,
        AudioBlock {
            id: BlockId("rig:input-7:block:split".into()),
            enabled: true,
            kind: AudioBlockKind::Split(SplitBlock {
                b: vec![amp],
                ..SplitBlock::new(SplitEnd::Mix)
            }),
        },
    );
    rig.inputs.get_mut(INPUT).expect("input-7").active_scene = 2;
    rig
}

#[test]
fn a_model_swap_inside_a_path_keeps_the_active_scene_on_the_live_block() {
    let rig = rig_with_the_amp_in_path_b();
    let project = Rc::new(RefCell::new(engine::rig_runtime::rig_to_legacy_project(
        &rig,
        &BTreeSet::new(),
    )));
    let dispatcher = LocalDispatcher::new(Rc::clone(&project));
    dispatcher.attach_rig(Rc::new(RefCell::new(rig)));

    dispatcher
        .dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
            chain: ChainId(CHAIN.into()),
            block: BlockId(AMP.into()),
            model_id: NEW_MODEL.into(),
        }))
        .expect("ReplaceBlockModel reaches the amp inside path B");

    let mut project = project.borrow_mut();
    let chain = project
        .chains
        .iter_mut()
        .find(|c| c.id.0 == CHAIN)
        .expect("the rig chain");
    let live = find_block_mut(&mut chain.blocks, AMP).expect("the amp is still in path B");
    let AudioBlockKind::Core(core) = &live.kind else {
        panic!("the amp is a core block");
    };
    assert_eq!(core.model, NEW_MODEL, "the live amp took the new model");
    assert_eq!(
        core.params.get_f32("input"),
        Some(67.0),
        "scene 2's surviving `input` override reaches the live amp"
    );
    assert_eq!(
        core.params.get_f32("output"),
        Some(41.0),
        "scene 2's surviving `output` override reaches the live amp"
    );
}
