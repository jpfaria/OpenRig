//! Red-first regression test for the live bug: "não consigo salvar
//! parâmetros na scene". Exercises the same path the GUI follows:
//! create a chain, add an effect block, switch to scene 2, edit a
//! param, dispatch CaptureRigEdits (which is what build_rig_for_save
//! runs before save_rig_project_file), and confirm the rig actually
//! holds the override.

use crate::project_ops::{
    create_new_project_session, load_project_session, save_project_session,
};
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::{Command, RigNavKind};
use application::dispatcher::CommandDispatcher;
use domain::ids::BlockId;
use std::path::PathBuf;
use tempfile::TempDir;

struct Sandbox {
    _tmp: TempDir,
    path: PathBuf,
    cfg: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("project.yaml");
        let cfg = tmp.path().join("config.yaml");
        Self {
            _tmp: tmp,
            path,
            cfg,
        }
    }
    fn new_session(&self) -> ProjectSession {
        let mut s = create_new_project_session(&self.cfg);
        s.project_path = Some(self.path.clone());
        s.config_path = Some(self.cfg.clone());
        s
    }
    fn save(&self, s: &ProjectSession) {
        save_project_session(s, &self.path).expect("save");
    }
    fn reload(&self) -> ProjectSession {
        load_project_session(&self.path, &self.cfg).expect("reload")
    }
}

fn setup_one_chain_with_gate(s: &Sandbox) -> (ProjectSession, BlockId) {
    let session = s.new_session();
    let chain = build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some("Chain".into()),
        input: EndpointSpec {
            device_id: Some("dev"),
            channels: vec![0],
        },
        output: EndpointSpec {
            device_id: Some("dev"),
            channels: vec![0, 1],
        },
    });
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");
    let chain_id = session.project.borrow().chains[0].id.clone();
    // Insert a gate_basic between input and output via AddBlock.
    session
        .dispatcher
        .dispatch(Command::AddBlock {
            chain: chain_id.clone(),
            kind: "dynamics".into(),
            model_id: "gate_basic".into(),
            position: 1,
        })
        .expect("AddBlock");
    // Pull the new block id from the project.
    let gate = session
        .project
        .borrow()
        .chains
        .iter()
        .find(|c| c.id == chain_id)
        .and_then(|c| {
            c.blocks
                .iter()
                .find(|b| matches!(b.kind, project::block::AudioBlockKind::Core(ref cb) if cb.effect_type == "dynamics"))
                .map(|b| b.id.clone())
        })
        .expect("gate block present");
    // Capture so the gate lands in the preset (otherwise the next
    // re-projection wipes it).
    session
        .dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("capture");
    (session, gate)
}

fn read_gate_threshold(session: &ProjectSession, block: &BlockId) -> Option<f32> {
    use project::block::AudioBlockKind;
    session
        .project
        .borrow()
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .find(|b| b.id == *block)
        .and_then(|b| match &b.kind {
            AudioBlockKind::Core(c) => c.params.get_f32("threshold"),
            _ => None,
        })
}

#[test]
fn scene_2_param_edit_persists_after_capture() {
    let s = Sandbox::new();
    let (session, gate) = setup_one_chain_with_gate(&s);
    let chain_id = session.project.borrow().chains[0].id.clone();

    // Add scene 2 (becomes active).
    session
        .dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: chain_id.clone(),
            kind: RigNavKind::Scene(-1),
        })
        .expect("add scene");

    // Edit threshold while on scene 2.
    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: gate.clone(),
            path: "threshold".into(),
            value: -55.0,
        })
        .expect("set threshold");
    session
        .dispatcher
        .dispatch(Command::CaptureRigEdits)
        .expect("capture");

    // The rig's scene 2 must carry the override.
    let rig = session.rig.as_ref().expect("rig").borrow();
    let preset_name = rig
        .inputs
        .values()
        .next()
        .and_then(|i| i.bank.get(&i.active_preset).cloned())
        .expect("active preset present");
    let preset = rig.presets.get(&preset_name).expect("preset");
    let key = format!("{}.threshold", gate.0);
    let s2 = preset
        .scenes
        .get(&2)
        .and_then(|sc| sc.params.get(&key).copied());
    assert_eq!(
        s2,
        Some(-55.0),
        "scene 2 must carry the override after CaptureRigEdits; \
         scenes={:?}, scene_params={:?}",
        preset.scenes,
        preset.scene_params,
    );
}

#[test]
fn scene_param_edit_survives_save_and_reload() {
    let s = Sandbox::new();
    let (session, gate) = setup_one_chain_with_gate(&s);
    let chain_id = session.project.borrow().chains[0].id.clone();

    // Switch on scene 2 and set the override.
    session
        .dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: chain_id.clone(),
            kind: RigNavKind::Scene(-1),
        })
        .expect("add scene");
    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id,
            block: gate.clone(),
            path: "threshold".into(),
            value: -55.0,
        })
        .expect("set threshold");

    s.save(&session);

    // Reload and switch to scene 2: the override must come back.
    let reloaded = s.reload();
    let new_chain_id = reloaded.project.borrow().chains[0].id.clone();
    reloaded
        .dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: new_chain_id,
            kind: RigNavKind::Scene(2),
        })
        .expect("switch to scene 2");

    let v = read_gate_threshold(&reloaded, &gate);
    assert_eq!(
        v,
        Some(-55.0),
        "after save+reload, scene 2's threshold override must \
         project back into the chain (got {v:?})"
    );
}

#[test]
fn scene_2_edit_does_not_overwrite_scene_1() {
    let s = Sandbox::new();
    let (session, gate) = setup_one_chain_with_gate(&s);
    let chain_id = session.project.borrow().chains[0].id.clone();

    // Scene 1 edit.
    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: gate.clone(),
            path: "threshold".into(),
            value: -20.0,
        })
        .expect("set s1");
    // Add scene 2 (snapshot from scene 1).
    session
        .dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: chain_id.clone(),
            kind: RigNavKind::Scene(-1),
        })
        .expect("add scene 2");
    // Edit on scene 2.
    session
        .dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id.clone(),
            block: gate.clone(),
            path: "threshold".into(),
            value: -77.0,
        })
        .expect("set s2");
    // Switch back to scene 1 — the original edit must still apply.
    session
        .dispatcher
        .dispatch(Command::ApplyRigNav {
            chain: chain_id,
            kind: RigNavKind::Scene(1),
        })
        .expect("switch to scene 1");

    let v = read_gate_threshold(&session, &gate);
    assert_eq!(
        v,
        Some(-20.0),
        "scene 1 edit must survive a scene-2 detour (got {v:?})"
    );
}
