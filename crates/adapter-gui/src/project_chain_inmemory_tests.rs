//! Red-first tests for in-memory chain creation. The user reported:
//! "criei uma chain nova e não veio com o preset preenchido nem a
//! scene 1." That means after `Command::AddChain` runs (and BEFORE
//! any save/reload), the session's `RigProject` must already have
//! the input + preset + scene 1 so the GUI's preset combobox is
//! populated immediately.
//!
//! Each test exercises the path the GUI follows: build a chain via
//! `chain_factory::build_default_chain`, dispatch `Command::AddChain`,
//! then assert on `session.rig` directly. No save, no reload.

use crate::project_ops::create_new_project_session;
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use project::chain::Chain;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_session(tmp: &TempDir) -> ProjectSession {
    let cfg: PathBuf = tmp.path().join("config.yaml");
    let path: PathBuf = tmp.path().join("project.yaml");
    let mut session = create_new_project_session(&cfg);
    session.project_path = Some(path);
    session.config_path = Some(cfg);
    session
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
// 1. New session already exposes a rig the GUI can render from
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_project_session_has_a_rig_attached() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    assert!(
        session.rig.is_some(),
        "the preset/scene UI binds to `session.rig`; a NEW session must \
         already have one so the GUI has something to render against"
    );
}

// ────────────────────────────────────────────────────────────────────
// 2. AddChain populates the rig with input + preset + scene 1
// ────────────────────────────────────────────────────────────────────

#[test]
fn add_chain_command_creates_a_rig_input() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    assert_eq!(
        rig.inputs.len(),
        1,
        "AddChain must add a corresponding input to the rig in memory; \
         got inputs={:?}",
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let (_, input) = rig.inputs.iter().next().unwrap();
    assert_eq!(input.label.as_deref(), Some("Chain 1"));
}

#[test]
fn add_chain_command_creates_a_preset_named_preset_1() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    let preset_names: Vec<Option<String>> = rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        preset_names
            .iter()
            .any(|n| n.as_deref() == Some("Preset 1")),
        "AddChain must create a default 'Preset 1' in the rig; got {preset_names:?}"
    );
    assert!(
        !preset_names.iter().any(|n| n.as_deref() == Some("Chain 1")),
        "preset.name must not duplicate the chain title"
    );
}

#[test]
fn add_chain_command_creates_scene_1_in_the_new_preset() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    for (preset_name, preset) in &rig.presets {
        assert!(
            preset.scenes.contains_key(&1),
            "preset {preset_name:?} should have scene 1 already created"
        );
    }
}

// ────────────────────────────────────────────────────────────────────
// 3. The active preset/scene are pointed at slot 1 so the UI's
//    combobox lights up "Preset 1" by default
// ────────────────────────────────────────────────────────────────────

#[test]
fn add_chain_active_preset_and_scene_default_to_1() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain })
        .expect("AddChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    let (_, input) = rig.inputs.iter().next().expect("one input");
    assert_eq!(input.active_preset, 1, "active preset defaults to slot 1");
    assert_eq!(input.active_scene, 1, "active scene defaults to 1");
    assert!(
        input.bank.contains_key(&1),
        "bank slot 1 holds the default preset key"
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. Two chains with the same source produce two independent rig inputs
// ────────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────────
// 5. The chain editor uses `Command::SaveChain` (upsert), not
//    `Command::AddChain`. So SaveChain must also populate the rig
//    when the chain is brand-new.
// ────────────────────────────────────────────────────────────────────

#[test]
fn save_chain_on_new_chain_id_creates_a_rig_input() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    assert_eq!(
        rig.inputs.len(),
        1,
        "SaveChain on a brand-new chain id must mirror into the rig; \
         got inputs={:?}",
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let (_, input) = rig.inputs.iter().next().unwrap();
    assert_eq!(input.label.as_deref(), Some("Chain 1"));
}

#[test]
fn save_chain_on_new_chain_creates_default_preset_and_scene() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    let preset_names: Vec<Option<String>> = rig.presets.values().map(|p| p.name.clone()).collect();
    assert!(
        preset_names
            .iter()
            .any(|n| n.as_deref() == Some("Preset 1")),
        "SaveChain must seed 'Preset 1'; got {preset_names:?}"
    );
    for (key, preset) in &rig.presets {
        assert!(
            preset.scenes.contains_key(&1),
            "preset {key:?} should have scene 1"
        );
    }
}

#[test]
fn save_chain_on_existing_chain_does_not_duplicate_rig_input() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("first save");

    // The first SaveChain re-tagged the chain id to `rig:<input>`.
    // Editing in the GUI dispatches SaveChain again with that same id —
    // the upsert branch must update in place and not duplicate the rig.
    let mut existing = session.project.borrow().chains[0].clone();
    existing.description = Some("Renamed".into());
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain: existing })
        .expect("second save");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    assert_eq!(
        rig.inputs.len(),
        1,
        "re-saving an existing chain must not duplicate the rig input \
         (got inputs={:?})",
        rig.inputs.keys().collect::<Vec<_>>()
    );
}

// ────────────────────────────────────────────────────────────────────
// 6. The chain's id must use the `rig:<input>` shape so the chains
//    screen's preset combobox (which only renders for `rig:` chains)
//    actually shows "Preset 1" instead of falling back to the empty
//    `RigNavRow::default()`.
// ────────────────────────────────────────────────────────────────────

// Reproduces the live bug: the chain-editor "save" callback cloned
// `chain.id` *before* dispatching SaveChain, then passed that pre-
// dispatch id to `sync_live_chain_runtime`. The dispatcher now
// re-tags the id to `rig:<input>` during the dispatch, so the
// callback's cached `chain_id` no longer exists in the project. The
// runtime lookup fails, the callback returns early, and the
// `refresh_chain_rig_nav` call that lights up the preset combobox
// never runs.
#[test]
fn pre_dispatch_chain_id_is_invalid_after_save_chain() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    let pre_dispatch_id = chain.id.clone();

    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");

    let still_present = session
        .project
        .borrow()
        .chains
        .iter()
        .any(|c| c.id == pre_dispatch_id);
    assert!(
        !still_present,
        "pre-dispatch chain id must NOT be the one stored after dispatch \
         (the dispatcher retags it to `rig:<input>`); callers must read \
         the post-dispatch id from the project, not cache the original"
    );
    // The chain that *is* present uses the `rig:` shape.
    let post = session.project.borrow().chains[0].id.clone();
    assert!(
        post.0.starts_with("rig:"),
        "post-dispatch id must be `rig:<input>`; got {:?}",
        post.0
    );
}

#[test]
fn save_chain_rewrites_chain_id_to_rig_prefix() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");

    let project = session.project.borrow();
    assert_eq!(project.chains.len(), 1);
    assert!(
        project.chains[0].id.0.starts_with("rig:"),
        "after SaveChain, chain.id must use the `rig:<input>` shape so \
         rig_nav_rows can locate it; got id={:?}",
        project.chains[0].id.0
    );
}

#[test]
fn rig_nav_rows_after_save_chain_carry_default_preset_label() {
    use crate::chain_rig_nav::rig_nav_rows;
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let chain = chain_with(&session, "Chain 1", "dev-A");
    session
        .dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain");

    let rig = session.rig.as_ref().expect("rig attached");
    let rows = rig_nav_rows(&rig.borrow(), &session.project.borrow());
    assert_eq!(rows.len(), 1, "one row per chain");
    let row = &rows[0];
    assert_eq!(
        row.preset_labels,
        vec!["Preset 1".to_string()],
        "combobox must show 'Preset 1' for a freshly-saved chain; got {row:?}"
    );
    assert_eq!(row.scene, 1, "default scene is 1");
    assert!(row.scene_count >= 1, "at least one scene must exist");
}

#[test]
fn two_add_chain_calls_with_same_source_produce_two_inputs() {
    let tmp = TempDir::new().unwrap();
    let session = new_session(&tmp);
    let a = chain_with(&session, "Chain 1", "shared-dev");
    let b = chain_with(&session, "Chain 2", "shared-dev");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain: a })
        .expect("AddChain A");
    session
        .dispatcher
        .dispatch(Command::AddChain { chain: b })
        .expect("AddChain B");

    let rig = session.rig.as_ref().expect("rig attached");
    let rig = rig.borrow();
    assert_eq!(
        rig.inputs.len(),
        2,
        "two AddChain dispatches with the same source must produce two \
         inputs (got {:?})",
        rig.inputs.keys().collect::<Vec<_>>()
    );
    let labels: Vec<Option<String>> = rig.inputs.values().map(|i| i.label.clone()).collect();
    assert!(
        labels.contains(&Some("Chain 1".to_string()))
            && labels.contains(&Some("Chain 2".to_string())),
        "both labels survive; got {labels:?}"
    );
}
