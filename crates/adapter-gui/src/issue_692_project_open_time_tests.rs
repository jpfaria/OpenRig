//! Issue #692 — opening a project takes far too long even though the
//! project file is tiny. Expected contract: project open is
//! parse-into-memory only; heavy DSP work (NAM weights, LV2
//! instantiation, IR convolver setup, stream rebuild) belongs to chain
//! start, not to open.
//!
//! Phase 1 probe (layer 1 of the open path): time `load_project_session`
//! against a rig the test generates in a tempdir. RED if this layer alone
//! blows the parse budget; GREEN here means the stall lives further down
//! the open wiring and the next probe moves there.
//!
//! The probe used to load the machine owner's live `~/.openrig`, so its
//! verdict depended on whatever the developer happened to be editing while
//! the suite ran — twice it reported a failure on data no branch had
//! touched. Everything it needs is generated here instead, at the scale
//! the #692 report was about, so the answer is the same on every machine.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use domain::ids::BlockId;
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock, NamBlock};
use project::param::ParameterSet;
use project::rig::{RigInput, RigOutput, RigPreset, RigProject, RigScene};
use tempfile::TempDir;

use crate::project_ops::load_project_session;

/// Generous ceiling for "read a rig document into memory". Parsing alone
/// sits in the microsecond range; anything past this budget means open is
/// doing work that belongs to chain start.
const OPEN_PARSE_BUDGET: Duration = Duration::from_millis(200);

// The rig #692 was reported on: several chains, each with a full preset
// bank of amp + cab + effect blocks and their scenes, serializing to a
// ~158 KB `.openrig`. The fixture is dimensioned to that document — a
// two-block project parses fast even when open is doing work it should
// not, so a small fixture would pin nothing.
const FIXTURE_CHAINS: usize = 4;
const FIXTURE_PRESETS_PER_CHAIN: usize = 5;
const FIXTURE_BLOCKS_PER_PRESET: usize = 10;
const FIXTURE_PARAMS_PER_BLOCK: usize = 10;
const FIXTURE_SCENES_PER_PRESET: usize = 3;
/// Floor on the generated document, so editing the dimensions above can
/// never quietly shrink the fixture back into a trivially fast one.
const MIN_FIXTURE_BYTES: u64 = 150 * 1024;

fn fixture_params(seed: usize) -> ParameterSet {
    let mut params = ParameterSet::default();
    for i in 0..FIXTURE_PARAMS_PER_BLOCK {
        params.insert(
            format!("param-{i:02}"),
            ParameterValue::Float((seed + i) as f32 * 0.125),
        );
    }
    params
}

/// One block of the preset, cycling the kinds the reported rig mixes:
/// a NAM amp capture, an IR cab, and a native effect.
fn fixture_block(preset_id: &str, idx: usize) -> AudioBlock {
    let kind = match idx % 3 {
        0 => AudioBlockKind::Nam(NamBlock {
            model: format!("fixture-amp-capture-{idx:02}"),
            params: fixture_params(idx),
        }),
        1 => AudioBlockKind::Core(CoreBlock {
            effect_type: "cabinet".into(),
            model: format!("fixture-ir-cab-{idx:02}"),
            params: fixture_params(idx),
        }),
        _ => AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: format!("fixture-delay-{idx:02}"),
            params: fixture_params(idx),
        }),
    };
    AudioBlock {
        id: BlockId(format!("{preset_id}:blk-{idx:02}")),
        enabled: true,
        kind,
    }
}

fn fixture_preset(id: &str) -> RigPreset {
    let blocks: Vec<AudioBlock> = (0..FIXTURE_BLOCKS_PER_PRESET)
        .map(|idx| fixture_block(id, idx))
        .collect();
    let scene_params: Vec<String> = blocks
        .iter()
        .map(|b| format!("{}.param-00", b.id.0))
        .collect();
    let scenes = (1..=FIXTURE_SCENES_PER_PRESET)
        .map(|scene| {
            (
                scene,
                RigScene {
                    label: Some(format!("Scene {scene}")),
                    bypass: blocks
                        .iter()
                        .map(|b| (b.id.0.clone(), scene % 2 == 0))
                        .collect(),
                    params: scene_params
                        .iter()
                        .map(|key| (key.clone(), scene as f32 * 0.1))
                        .collect(),
                    volume: Some(100.0),
                },
            )
        })
        .collect();
    RigPreset {
        id: id.to_string(),
        name: Some(format!("Preset {id}")),
        blocks,
        scene_params,
        scenes,
        volume: 100.0,
    }
}

/// Write the fixture rig into `dir` and return its path.
fn write_fixture_rig(dir: &Path) -> PathBuf {
    let mut presets = BTreeMap::new();
    let mut inputs = BTreeMap::new();
    for chain in 0..FIXTURE_CHAINS {
        let mut bank = BTreeMap::new();
        for slot in 1..=FIXTURE_PRESETS_PER_CHAIN {
            let id = format!("chain-{chain}-preset-{slot:02}");
            presets.insert(id.clone(), fixture_preset(&id));
            bank.insert(slot, id);
        }
        inputs.insert(
            format!("input-{chain}"),
            RigInput {
                label: Some(format!("Guitar {chain}")),
                bank,
                active_preset: 1,
                active_scene: 1,
                routing: vec!["main".to_string()],
                instrument: "electric_guitar".to_string(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
            },
        );
    }
    let mut outputs = BTreeMap::new();
    outputs.insert(
        "main".to_string(),
        RigOutput {
            label: Some("Main".to_string()),
            io: "fixture-binding".to_string(),
            endpoint: "main".to_string(),
        },
    );
    let rig = RigProject {
        name: Some("issue-692 fixture".to_string()),
        inputs,
        outputs,
        presets,
        midi: None,
        chain_order: Vec::new(),
    };
    let path = dir.join("project.openrig");
    infra_yaml::save_rig_project_file(&path, &rig).expect("write fixture rig");
    path
}

#[test]
fn issue_692_load_project_session_stays_within_parse_budget() {
    let tmp = TempDir::new().expect("tempdir");
    let project_path = write_fixture_rig(tmp.path());
    let size = std::fs::metadata(&project_path)
        .expect("fixture metadata")
        .len();
    assert!(
        size >= MIN_FIXTURE_BYTES,
        "fixture is only {size} bytes; #692 was reported on a ~158 KB rig, \
         and a small document would make this budget vacuous"
    );
    // No config.yaml in the tempdir: open must not need one, and reading
    // the machine's own config is not part of what this probe measures.
    let config_path = tmp.path().join("config.yaml");

    let t0 = Instant::now();
    let session = load_project_session(&project_path, &config_path).expect("load_project_session");
    let elapsed = t0.elapsed();
    eprintln!(
        "issue_692: load_project_session took {elapsed:?} for a {size}-byte rig ({} chains)",
        session.project.borrow().chains.len()
    );
    assert_eq!(
        session.project.borrow().chains.len(),
        FIXTURE_CHAINS,
        "the whole fixture must have been projected, otherwise the timing \
         is measuring a rig that never loaded"
    );

    assert!(
        elapsed < OPEN_PARSE_BUDGET,
        "project open (load_project_session) took {elapsed:?}, budget is {OPEN_PARSE_BUDGET:?} — \
         open must be parse-only; heavy DSP setup belongs to chain start (issue #692)"
    );
}

/// Phase 1 probe (layer 2 of the open path): time the full runtime start
/// (`ProjectRuntimeController::start`) — the NAM/LV2/IR build + stream
/// bring-up that runs when chains go live. RED if the whole bring-up
/// exceeds the budget, localizing the user-visible stall.
///
/// This one measures real capture loading, so it needs a real rig and the
/// capture library it references. Both are named explicitly by the
/// operator (`OPENRIG_OWNER_PROJECT` / `OPENRIG_OWNER_PLUGINS`, as the rest
/// of the owner-recipe battery does) rather than guessed from `$HOME`: a
/// test must never reach into the developer's live files on its own.
#[test]
fn issue_692_runtime_start_probe() {
    // Real-hardware battery gate: opens the physical audio interface.
    if std::env::var_os("OPENRIG_HW_TESTS").is_none() {
        eprintln!("issue_692: SKIPPED — set OPENRIG_HW_TESTS=1 on an idle machine");
        return;
    }
    let (Some(owner_project), Some(owner_plugins)) = (
        std::env::var_os("OPENRIG_OWNER_PROJECT"),
        std::env::var_os("OPENRIG_OWNER_PLUGINS"),
    ) else {
        eprintln!(
            "issue_692: SKIPPED — needs OPENRIG_OWNER_PROJECT=<project.openrig> and \
             OPENRIG_OWNER_PLUGINS=<OpenRig-plugins/plugins/source>"
        );
        return;
    };
    let owner_project = PathBuf::from(owner_project);
    let owner_plugins = PathBuf::from(owner_plugins);
    if !owner_project.exists() || !owner_plugins.exists() {
        eprintln!("issue_692: SKIPPED — owner project or capture tree not present");
        return;
    }

    let t0 = Instant::now();
    plugin_loader::registry::init_many(&[PathBuf::from("plugins"), owner_plugins]);
    eprintln!("issue_692: registry init took {:?}", t0.elapsed());

    // Copy before loading: the probe must never mutate the operator's file.
    let tmp = TempDir::new().expect("tempdir");
    let project_path = tmp.path().join("project.openrig");
    std::fs::copy(&owner_project, &project_path).expect("copy owner project");
    let config_path = tmp.path().join("config.yaml");

    let session = load_project_session(&project_path, &config_path).expect("load_project_session");
    let mut project = session.project.borrow().clone();
    // Mirror the user flipping every chain on (the rig going live).
    for chain in &mut project.chains {
        chain.enabled = true;
    }
    let enabled_chains = project.chains.iter().filter(|c| c.enabled).count();

    let t1 = Instant::now();
    let controller = infra_cpal::ProjectRuntimeController::start(&project).expect("runtime start");
    let elapsed = t1.elapsed();
    eprintln!(
        "issue_692: ProjectRuntimeController::start took {elapsed:?} \
         ({enabled_chains} enabled chains of {})",
        project.chains.len()
    );
    drop(controller);

    assert!(
        elapsed < Duration::from_millis(500),
        "runtime start took {elapsed:?} for {enabled_chains} enabled chain(s) — \
         this is the user-visible stall when opening/starting the rig (issue #692)"
    );
}
