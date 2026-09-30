//! #986 — the project file is written ONLY on an explicit save.
//!
//! Measured on the owner's machine: `project.yaml` was rewritten 10 times in
//! 30 minutes with no save, so a destructive edit reached disk at once and
//! reopening the project could not undo it. The owner's decision: no
//! autosave. Editing — adding a scene, changing a parameter, changing a
//! model, capturing the rig edits — leaves the file untouched; `SaveProject`
//! (the GUI Save button and the MCP `save_project` tool dispatch the same
//! command) is what writes it.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use application::command::{BlockCommand, Command, ProjectCommand, RigNavKind, SelectionCommand};
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::rig::{RigInput, RigPreset, RigProject};

use crate::state::ProjectSession;

const CHAIN: &str = "rig:in";
const AMP: &str = "rig:in:block:amp";

fn amp_block() -> AudioBlock {
    AudioBlock {
        id: BlockId(AMP.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "amp".into(),
            model: "blackface_clean".into(),
            params: application::block_factory::default_params_for_model("amp", "blackface_clean")
                .expect("native amp params"),
        }),
    }
}

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "p1".into(),
        RigPreset::from_legacy_blocks(vec![amp_block()], 100.0),
    );
    let mut bank = BTreeMap::new();
    bank.insert(1, "p1".into());
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "in".into(),
        RigInput {
            label: Some("GUITARRA".into()),
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: Vec::new(),
            loopers: Vec::new(),
            mix: Default::default(),
        },
    );
    RigProject {
        name: Some("project".into()),
        inputs,
        presets,
        outputs: BTreeMap::new(),
        chain_order: Vec::new(),
        midi: None,
    }
}

/// Open the project off disk the way the app does, with the real
/// `RuntimeControl` attached so every side effect a command emits runs.
fn open(project_path: &Path) -> ProjectSession {
    let (rig, project) =
        crate::project_ops::load_rig_and_project(project_path).expect("open the project");
    let mut session = ProjectSession::new(
        project,
        Some(project_path.to_path_buf()),
        None,
        PathBuf::from("./presets"),
    );
    let rig = Rc::new(RefCell::new(rig));
    session.dispatcher.attach_rig(Rc::clone(&rig));
    session.rig = Some(rig);
    crate::runtime_lifecycle::attach_runtime_control(
        &Rc::new(RefCell::new(None)),
        &crate::runtime_analyzers::AnalyzerSessions::detached(),
        &session,
    );
    session
}

fn on_disk(path: &Path) -> String {
    application::persist_worker::flush();
    std::fs::read_to_string(path).expect("read the project file")
}

fn edit_without_saving(session: &ProjectSession) {
    let chain = ChainId(CHAIN.into());
    let d = &session.dispatcher;
    d.dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
        chain: chain.clone(),
        kind: RigNavKind::Scene(-1),
    }))
    .expect("add a scene");
    d.dispatch(Command::Block(BlockCommand::SetBlockParameterNumber {
        chain: chain.clone(),
        block: BlockId(AMP.into()),
        path: "gain".into(),
        value: 71.0,
    }))
    .expect("set a parameter");
    d.dispatch(Command::Block(BlockCommand::ReplaceBlockModel {
        chain: chain.clone(),
        block: BlockId(AMP.into()),
        model_id: "chime".into(),
    }))
    .expect("change the model");
    d.dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
        chain,
        kind: RigNavKind::Scene(1),
    }))
    .expect("switch scene");
    d.dispatch(Command::Project(ProjectCommand::CaptureRigEdits))
        .expect("capture the rig edits");
}

#[test]
fn editing_the_project_never_writes_the_project_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("project.yaml");
    infra_yaml::save_rig_project_file(&path, &rig()).expect("write the project");
    let before = on_disk(&path);
    let session = open(&path);

    edit_without_saving(&session);

    assert_eq!(
        on_disk(&path),
        before,
        "#986: no autosave — edits must stay in memory until the user saves"
    );
}

#[test]
fn save_project_writes_the_edits() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("project.yaml");
    infra_yaml::save_rig_project_file(&path, &rig()).expect("write the project");
    let session = open(&path);
    edit_without_saving(&session);

    session
        .dispatcher
        .dispatch(Command::Project(ProjectCommand::SaveProject))
        .expect("save");
    application::persist_worker::flush();

    let saved = infra_yaml::load_rig_project_file(&path).expect("reload");
    let preset = &saved.presets["p1"];
    assert_eq!(preset.scenes.len(), 2, "the added scene reached disk");
    let model = match &preset.blocks[0].kind {
        AudioBlockKind::Core(core) => core.model.clone(),
        other => panic!("amp stays core, got {}", other.label()),
    };
    assert_eq!(model, "chime", "the model change reached disk on save");
}
