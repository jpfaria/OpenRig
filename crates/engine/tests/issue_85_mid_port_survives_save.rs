//! #85 — the user places a mid `Output`, saves, reopens: the port is gone.
//!
//! Save runs `sync_synthetic_into_rig` (chain blocks → rig preset) and reopen
//! runs `rig_to_legacy_project` (rig preset → chain blocks). A mid port has to
//! come back at the very index the user left it, still pointing at its E/S.

use std::collections::{BTreeMap, BTreeSet};

use domain::ids::{BlockId, ChainId};
use engine::rig_runtime::rig_to_legacy_project;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use project::chain::Chain;
use project::param::ParameterSet;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};
use project::rig_sync::sync_synthetic_into_rig;

fn core(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn mid_output() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-1:output:1".into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "Out 1".into(),
        }),
    }
}

fn mid_input() -> AudioBlock {
    AudioBlock {
        id: BlockId("rig:input-1:input:1".into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: "aux".into(),
            endpoint: "In 1".into(),
        }),
    }
}

fn rig() -> RigProject {
    let mut presets = BTreeMap::new();
    presets.insert(
        "clean".to_string(),
        RigPreset {
            id: String::new(),
            name: None,
            blocks: vec![],
            scene_params: vec![],
            scenes: BTreeMap::new(),
            volume: 100.0,
        },
    );
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: None,
            bank: BTreeMap::from([(1, "clean".to_string())]),
            active_preset: 1,
            active_scene: 1,
            routing: vec![],
            instrument: "electric_guitar".to_string(),
            io: String::new(),
            endpoint: String::new(),
            io_binding_ids: vec!["main".to_string()],
            loopers: Vec::new(),
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

fn project_with(blocks: Vec<AudioBlock>) -> Project {
    let mut project = Project::default();
    project.chains = vec![Chain {
        id: ChainId("rig:input-1".into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: true,
        volume: 100.0,
        io_binding_ids: vec!["main".to_string()],
        blocks,
        di_output: None,
        loopers: vec![],
    }];
    project
}

/// Save then reopen, the way the GUI does it.
fn round_trip(blocks: Vec<AudioBlock>) -> Vec<AudioBlock> {
    let mut rig = rig();
    sync_synthetic_into_rig(&mut rig, &project_with(blocks));
    let reopened = rig_to_legacy_project(&rig, &BTreeSet::new());
    reopened.chains[0].blocks.clone()
}

#[test]
fn a_mid_output_survives_save_and_reopen_at_its_position() {
    let blocks = round_trip(vec![core("A"), mid_output(), core("B")]);

    let kinds: Vec<&str> = blocks.iter().map(|b| b.kind.label()).collect();
    assert_eq!(
        kinds.get(1).copied(),
        Some("output"),
        "#85: the mid Output must come back between A and B (got {kinds:?})"
    );
    match &blocks[1].kind {
        AudioBlockKind::Output(o) => {
            assert_eq!((o.io.as_str(), o.endpoint.as_str()), ("aux", "Out 1"))
        }
        other => panic!("expected the Output port, got {}", other.label()),
    }
}

#[test]
fn a_mid_input_survives_save_and_reopen_at_its_position() {
    let blocks = round_trip(vec![core("A"), mid_input(), core("B")]);

    let kinds: Vec<&str> = blocks.iter().map(|b| b.kind.label()).collect();
    assert_eq!(
        kinds.get(1).copied(),
        Some("input"),
        "#85: the mid Input must come back between A and B (got {kinds:?})"
    );
}

/// Re-pointing a port that is ALREADY in the preset: the block list does not
/// change shape, so save takes the per-scene write-back path — which only ever
/// captured params and bypass. The new E/S was dropped on the floor: the port
/// kept its original binding in `project.yaml` and came back wrong on reopen.
#[test]
fn repointing_a_mid_port_persists_its_new_endpoint() {
    let mut rig = rig();
    // The preset already holds the port, bound to `aux` / `Out 1`.
    rig.presets.get_mut("clean").unwrap().blocks = vec![core("A"), mid_output(), core("B")];

    // The user picks another E/S in the port editor.
    let mut repointed = mid_output();
    repointed.kind = AudioBlockKind::Output(OutputBlock {
        model: "standard".into(),
        io: "aux2".into(),
        endpoint: "Aux 2 out".into(),
    });
    sync_synthetic_into_rig(
        &mut rig,
        &project_with(vec![core("A"), repointed, core("B")]),
    );

    match &rig.presets["clean"].blocks[1].kind {
        AudioBlockKind::Output(o) => assert_eq!(
            (o.io.as_str(), o.endpoint.as_str()),
            ("aux2", "Aux 2 out"),
            "#85: picking another E/S for an existing port must reach the preset"
        ),
        other => panic!("expected the Output port, got {}", other.label()),
    }
}
