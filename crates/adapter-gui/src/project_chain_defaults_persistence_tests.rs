//! Red-first tests for the chain-creation defaults the user wants:
//!
//! 1. A newly created chain saves with `input.label` set to the chain
//!    description, but `preset.name` is a distinct default (e.g.
//!    "Preset 1") — never a duplicate of the chain name.
//! 2. The first scene exists explicitly in `preset.scenes` so the
//!    user can edit scene 1 without having to "create" it first.
//! 3. Two chains that share the same capture source do NOT collapse
//!    into one rig input — each chain is its own input with its own
//!    preset bank. (The legacy auto-grouping is the source of the
//!    "input config is of the preset" confusion.)
//!
//! All tests exercise the same `create_new_project_session` →
//! `save_project_session` → `load_rig_project_file` path the GUI
//! follows. We read the on-disk `.openrig` directly so the assertions
//! are about what was persisted, not about the in-memory rig.

use crate::project_ops::{create_new_project_session, save_project_session};
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use project::chain::Chain;
use std::path::PathBuf;
use tempfile::TempDir;

struct Sandbox {
    _tmp: TempDir,
    path: PathBuf,
    openrig: PathBuf,
    cfg: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("project.yaml");
        let openrig = path.with_extension("openrig");
        let cfg = tmp.path().join("config.yaml");
        Self {
            _tmp: tmp,
            path,
            openrig,
            cfg,
        }
    }

    fn new_session(&self) -> ProjectSession {
        let mut s = create_new_project_session(&self.cfg);
        s.project_path = Some(self.path.clone());
        s.config_path = Some(self.cfg.clone());
        s
    }

    fn save(&self, session: &ProjectSession) {
        save_project_session(session, &self.path).expect("save");
    }

    fn read_openrig(&self) -> project::rig::RigProject {
        infra_yaml::load_rig_project_file(&self.openrig).expect("load .openrig")
    }
}

fn chain_with(session: &ProjectSession, desc: &str, dev: &str) -> Chain {
    build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some(desc.into()),
        input: EndpointSpec {
            device_id: Some(dev),
            channels: vec![0],
        },
        output: EndpointSpec {
            device_id: Some("test-out"),
            channels: vec![0, 1],
        },
    })
}

// ────────────────────────────────────────────────────────────────────
// 1. Chain name vs preset name
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_chain_save_sets_input_label_to_chain_description() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    let labels: Vec<Option<String>> =
        rig.inputs.values().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&Some("Chain 1".to_string())),
        "input.label must carry the chain description; got {labels:?}"
    );
}

#[test]
fn new_chain_save_does_not_duplicate_chain_name_into_preset_name() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    let preset_names: Vec<Option<String>> =
        rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        !preset_names.iter().any(|n| n.as_deref() == Some("Chain 1")),
        "preset.name must be distinct from the chain name; got {preset_names:?}"
    );
}

#[test]
fn new_chain_save_uses_default_preset_label() {
    // The preset should ship with a sensible default name so the user
    // can see something in the preset combobox; "Preset 1" is the
    // first slot of a brand-new chain's bank.
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    let preset_names: Vec<Option<String>> =
        rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        preset_names.iter().any(|n| n.as_deref() == Some("Preset 1")),
        "expected a 'Preset 1' default; got {preset_names:?}"
    );
}

// ────────────────────────────────────────────────────────────────────
// 2. Scene 1 must exist explicitly so it can be edited
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_chain_save_creates_scene_1_explicitly() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "Chain 1", "dev-A");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    // For every preset on the rig, scene 1 must exist as an entry of
    // `preset.scenes` (an empty `RigScene` is fine — the point is the
    // slot is editable, not requiring a "Create scene" action).
    for (preset_name, preset) in &rig.presets {
        assert!(
            preset.scenes.contains_key(&1),
            "preset {preset_name:?} is missing scene 1 (scenes: {:?})",
            preset.scenes.keys().collect::<Vec<_>>()
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// 3. Two chains sharing the same source must NOT collapse into one input
// ────────────────────────────────────────────────────────────────────

#[test]
fn two_chains_with_same_source_save_as_two_separate_inputs() {
    let s = Sandbox::new();
    let session = s.new_session();
    // Both chains tap the same Scarlett channel — under the old
    // grouping behaviour this collapses into ONE input with TWO
    // presets. The fix is: each chain is its own input.
    let a = chain_with(&session, "Chain 1", "shared-dev");
    let b = chain_with(&session, "Chain 2", "shared-dev");
    session.project.borrow_mut().chains.push(a);
    session.project.borrow_mut().chains.push(b);
    s.save(&session);

    let rig = s.read_openrig();
    assert_eq!(
        rig.inputs.len(),
        2,
        "two distinct chains must produce two distinct inputs, not be \
         grouped (got {} input(s): {:?})",
        rig.inputs.len(),
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let labels: Vec<Option<String>> =
        rig.inputs.values().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&Some("Chain 1".to_string()))
            && labels.contains(&Some("Chain 2".to_string())),
        "both chain labels must survive into separate inputs; got {labels:?}"
    );
}

#[test]
fn two_chains_with_same_source_save_as_two_independent_preset_pools() {
    // Stronger variant: each chain must own its own preset bank,
    // not share one. Two inputs ⇒ two banks ⇒ two presets total.
    let s = Sandbox::new();
    let session = s.new_session();
    let a = chain_with(&session, "Chain 1", "shared-dev");
    let b = chain_with(&session, "Chain 2", "shared-dev");
    session.project.borrow_mut().chains.push(a);
    session.project.borrow_mut().chains.push(b);
    s.save(&session);

    let rig = s.read_openrig();
    let total_bank_entries: usize =
        rig.inputs.values().map(|i| i.bank.len()).sum();
    assert_eq!(
        total_bank_entries,
        2,
        "each chain owns its own bank (expected 2 total slots, got {total_bank_entries}; \
         inputs: {:?})",
        rig.inputs
            .iter()
            .map(|(k, i)| (k.clone(), i.bank.clone()))
            .collect::<Vec<_>>()
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. Consistency check across all three defaults
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_chain_defaults_are_consistent_end_to_end() {
    let s = Sandbox::new();
    let session = s.new_session();
    let c = chain_with(&session, "GUITAR", "dev-X");
    session.project.borrow_mut().chains.push(c);
    s.save(&session);

    let rig = s.read_openrig();
    assert_eq!(rig.inputs.len(), 1, "one chain → one input");
    let (input_name, input) = rig.inputs.iter().next().unwrap();
    assert_eq!(input.label.as_deref(), Some("GUITAR"), "input.label is chain name");
    assert_eq!(input.bank.len(), 1, "exactly one preset slot");
    let preset_key = input.bank.get(&1).expect("bank slot 1 present");
    let preset = rig.presets.get(preset_key).expect("preset exists");
    assert_eq!(
        preset.name.as_deref(),
        Some("Preset 1"),
        "preset.name is the default 'Preset 1', not the chain name (input {input_name:?})"
    );
    assert!(
        preset.scenes.contains_key(&1),
        "scene 1 is present on the new preset"
    );
}
