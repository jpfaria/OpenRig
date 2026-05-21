//! Regression tests for the new-project save/reload round-trip
//! (bug: chains created in a new project disappear on exit/reopen).
//!
//! Reproduces the exact filesystem flows the user can trigger from the
//! GUI: creating a brand-new project, opening one from the recents list,
//! and the delete-all-then-add-new sequence that revealed the bug.
//! Tests are pure (no `AppWindow`); they only exercise the
//! `create_new_project_session` / `load_project_session` /
//! `save_project_session` triplet in `project_ops`, which is the same
//! path the GUI callbacks ultimately call.
//!
//! Note on chain identity: after a save/reload through the rig path,
//! chains are re-projected from the `RigProject` and carry synthetic
//! ids of the form `rig:<input>`. The user-facing identity is the
//! `description` (what the chain title shows), so assertions in this
//! file are written against `description` and chain count — never the
//! raw `ChainId`, which is an internal projection artefact.

use crate::project_ops::{
    create_new_project_session, load_project_session, save_project_session,
};
use crate::state::ProjectSession;
use application::chain_factory::{build_default_chain, DefaultChainParams, EndpointSpec};
use project::chain::Chain;
use std::path::PathBuf;
use tempfile::TempDir;

/// Build a chain with a unique capture source so each one becomes a
/// distinct rig input on migration (so multi-chain projects survive
/// the round-trip as multiple chains, not as N presets of one input).
fn chain_for(session: &ProjectSession, desc: &str) -> Chain {
    build_default_chain(DefaultChainParams {
        project: &session.project.borrow(),
        instrument: "electric_guitar",
        description: Some(desc.into()),
        input: EndpointSpec {
            device_id: Some(&format!("dev-{desc}")),
            channels: vec![0],
        },
        output: EndpointSpec {
            device_id: Some("test-out"),
            channels: vec![0, 1],
        },
    })
}

fn chain_descriptions(s: &ProjectSession) -> Vec<Option<String>> {
    s.project
        .borrow()
        .chains
        .iter()
        .map(|c| c.description.clone())
        .collect()
}

fn chain_count(s: &ProjectSession) -> usize {
    s.project.borrow().chains.len()
}

/// Set up a fresh in-memory session and bind it to a temp `project.yaml`,
/// mimicking what `on_confirm_new_project` does after the file-save
/// dialog picks a path.
fn new_session_at(path: &PathBuf, cfg: &PathBuf) -> ProjectSession {
    let mut session = create_new_project_session(cfg);
    session.project_path = Some(path.clone());
    session.config_path = Some(cfg.clone());
    session
}

// ────────────────────────────────────────────────────────────────────
// 1. Brand-new project flow
// ────────────────────────────────────────────────────────────────────

#[test]
fn new_project_save_then_reload_keeps_chain() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let session = new_session_at(&path, &cfg);
    let chain = chain_for(&session, "g1");
    session.project.borrow_mut().chains.push(chain);
    save_project_session(&session, &path).expect("save");

    let reloaded = load_project_session(&path, &cfg).expect("reload");
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("g1".to_string())],
        "chain lost after new-project save+reload"
    );
}

#[test]
fn new_project_save_then_reload_preserves_chain_description() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let session = new_session_at(&path, &cfg);
    let chain = chain_for(&session, "GUITARRA 1");
    session.project.borrow_mut().chains.push(chain);
    save_project_session(&session, &path).expect("save");

    let reloaded = load_project_session(&path, &cfg).expect("reload");
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("GUITARRA 1".to_string())],
        "chain description lost on reload"
    );
}

#[test]
fn new_project_save_then_reload_keeps_multiple_chains() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let session = new_session_at(&path, &cfg);
    for name in ["a", "b", "c"] {
        let c = chain_for(&session, name);
        session.project.borrow_mut().chains.push(c);
    }
    save_project_session(&session, &path).expect("save");

    let reloaded = load_project_session(&path, &cfg).expect("reload");
    assert_eq!(chain_count(&reloaded), 3, "expected 3 chains after reload");
    let descs = chain_descriptions(&reloaded);
    for name in ["a", "b", "c"] {
        assert!(
            descs.iter().any(|d| d.as_deref() == Some(name)),
            "missing chain {name} in {descs:?}"
        );
    }
}

#[test]
fn new_project_empty_save_then_reload_is_empty() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let session = new_session_at(&path, &cfg);
    save_project_session(&session, &path).expect("save");

    let reloaded = load_project_session(&path, &cfg).expect("reload");
    assert_eq!(chain_count(&reloaded), 0);
}

// ────────────────────────────────────────────────────────────────────
// 2. Opening an existing project from the recents list
// ────────────────────────────────────────────────────────────────────

#[test]
fn existing_project_reload_then_add_chain_persists_on_save() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // Seed disk with one chain.
    let seed = new_session_at(&path, &cfg);
    let c = chain_for(&seed, "seed");
    seed.project.borrow_mut().chains.push(c);
    save_project_session(&seed, &path).expect("seed save");

    // Reopen, add another chain, save, reopen again.
    let session = load_project_session(&path, &cfg).expect("reload 1");
    let c = chain_for(&session, "added");
    session.project.borrow_mut().chains.push(c);
    save_project_session(&session, &path).expect("save 2");

    let reloaded = load_project_session(&path, &cfg).expect("reload 2");
    let descs = chain_descriptions(&reloaded);
    assert_eq!(descs.len(), 2, "expected 2 chains, got {descs:?}");
    assert!(descs.iter().any(|d| d.as_deref() == Some("seed")));
    assert!(descs.iter().any(|d| d.as_deref() == Some("added")));
}

#[test]
fn existing_project_multiple_reload_cycles_are_stable() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let seed = new_session_at(&path, &cfg);
    let c = chain_for(&seed, "g1");
    seed.project.borrow_mut().chains.push(c);
    save_project_session(&seed, &path).expect("seed save");

    for cycle in 0..5 {
        let s = load_project_session(&path, &cfg).expect("reload");
        assert_eq!(
            chain_descriptions(&s),
            vec![Some("g1".to_string())],
            "chain drifted on cycle {cycle}"
        );
        save_project_session(&s, &path).expect("re-save");
    }
}

// ────────────────────────────────────────────────────────────────────
// 3. The exact sequence the user reported
// ────────────────────────────────────────────────────────────────────

#[test]
fn delete_all_chains_save_then_reload_persists_empty() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let seed = new_session_at(&path, &cfg);
    let ca = chain_for(&seed, "a");
    let cb = chain_for(&seed, "b");
    seed.project.borrow_mut().chains.push(ca);
    seed.project.borrow_mut().chains.push(cb);
    save_project_session(&seed, &path).expect("seed save");

    let s = load_project_session(&path, &cfg).expect("reload");
    s.project.borrow_mut().chains.clear();
    save_project_session(&s, &path).expect("save after delete");

    let reloaded = load_project_session(&path, &cfg).expect("reload 2");
    assert_eq!(
        chain_count(&reloaded),
        0,
        "delete-all did not persist (got {:?})",
        chain_descriptions(&reloaded)
    );
}

#[test]
fn delete_all_then_add_new_chain_save_reload_keeps_new() {
    // The exact sequence the user reported:
    //   1. project has chains, user opens it
    //   2. deletes all chains, saves, closes
    //   3. reopens, creates a new chain, saves, closes
    //   4. reopens — the new chain MUST be there.
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // Step 1: seed disk with two chains.
    let seed = new_session_at(&path, &cfg);
    let ca = chain_for(&seed, "a");
    let cb = chain_for(&seed, "b");
    seed.project.borrow_mut().chains.push(ca);
    seed.project.borrow_mut().chains.push(cb);
    save_project_session(&seed, &path).expect("seed save");

    // Step 2: open, delete all, save, close.
    let s = load_project_session(&path, &cfg).expect("reload 1");
    s.project.borrow_mut().chains.clear();
    save_project_session(&s, &path).expect("save after delete");
    drop(s);

    // Step 3: reopen, add a new chain, save, close.
    let s = load_project_session(&path, &cfg).expect("reload 2");
    let new_chain = chain_for(&s, "GUITARRA 1");
    s.project.borrow_mut().chains.push(new_chain);
    save_project_session(&s, &path).expect("save after add");
    drop(s);

    // Step 4: reopen — the new chain must be visible.
    let s = load_project_session(&path, &cfg).expect("reload 3");
    assert_eq!(
        chain_descriptions(&s),
        vec![Some("GUITARRA 1".to_string())],
        "the chain added after delete-all disappeared on reload"
    );
}

// ────────────────────────────────────────────────────────────────────
// 4. The disk-state corner case behind the bug
// ────────────────────────────────────────────────────────────────────

#[test]
fn stale_empty_openrig_does_not_swallow_recent_yaml_edits() {
    // Disk state observed in the wild on the affected machine:
    //   ~/.openrig/project.openrig  — old, empty (no chains)
    //   ~/.openrig/project.yaml     — recent, with the user's chain
    // The loader must reflect the recent `.yaml`, not the stale `.openrig`.
    let tmp = TempDir::new().unwrap();
    let yaml: PathBuf = tmp.path().join("project.yaml");
    let openrig: PathBuf = tmp.path().join("project.openrig");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // A pre-existing empty `.openrig` from a previous migration.
    std::fs::write(
        &openrig,
        "version: 1\nproject:\n  name: project\n  inputs: {}\n  outputs: {}\n  presets: {}\n",
    )
    .unwrap();

    // A freshly saved `.yaml` with a chain.
    let session = new_session_at(&yaml, &cfg);
    let c = chain_for(&session, "g1");
    session.project.borrow_mut().chains.push(c);
    save_project_session(&session, &yaml).expect("save");

    let reloaded = load_project_session(&yaml, &cfg).expect("reload");
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("g1".to_string())],
        "a stale `.openrig` swallowed the most recent `.yaml` save"
    );
}

#[test]
fn stale_openrig_with_other_chain_does_not_replace_recent_yaml() {
    // Stronger variant: the stale `.openrig` is *not* empty, it has a
    // chain that the user already removed. Re-loading must not resurrect
    // it from the stale sibling.
    let tmp = TempDir::new().unwrap();
    let yaml: PathBuf = tmp.path().join("project.yaml");
    let openrig: PathBuf = tmp.path().join("project.openrig");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // Stale `.openrig` with a ghost input + preset.
    std::fs::write(
        &openrig,
        "version: 1\nproject:\n  name: project\n  inputs:\n    ghost:\n      sources: []\n      bank: {1: ghost-preset}\n      active-preset: 1\n      active-scene: 1\n      routing: []\n  outputs: {}\n  presets:\n    ghost-preset:\n      id: ghost-preset\n      name: GHOST\n      volume: 100.0\n      blocks: []\n      scenes: {}\n      scene_params: []\n",
    )
    .unwrap();

    let session = new_session_at(&yaml, &cfg);
    let c = chain_for(&session, "real");
    session.project.borrow_mut().chains.push(c);
    save_project_session(&session, &yaml).expect("save");

    let reloaded = load_project_session(&yaml, &cfg).expect("reload");
    let descs = chain_descriptions(&reloaded);
    assert!(
        descs.iter().any(|d| d.as_deref() == Some("real")),
        "stale .openrig hid the freshly-saved chain (got {descs:?})"
    );
    assert!(
        !descs.iter().any(|d| d.as_deref() == Some("GHOST")),
        "stale .openrig resurrected a removed chain (got {descs:?})"
    );
}

// ────────────────────────────────────────────────────────────────────
// 5. Save-path / filename edge cases
// ────────────────────────────────────────────────────────────────────

#[test]
fn save_creates_parent_directory_when_missing() {
    let tmp = TempDir::new().unwrap();
    let nested: PathBuf = tmp.path().join("a/b/c/project.yaml");
    let cfg: PathBuf = tmp.path().join("a/b/c/config.yaml");

    let session = new_session_at(&nested, &cfg);
    let c = chain_for(&session, "g1");
    session.project.borrow_mut().chains.push(c);

    save_project_session(&session, &nested).expect("save creates dirs");

    let reloaded = load_project_session(&nested, &cfg).expect("reload");
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("g1".to_string())]
    );
}

#[test]
fn save_then_reload_works_with_openrig_extension() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.openrig");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let session = new_session_at(&path, &cfg);
    let c = chain_for(&session, "g1");
    session.project.borrow_mut().chains.push(c);

    save_project_session(&session, &path).expect("save .openrig");

    let reloaded = load_project_session(&path, &cfg).expect("reload .openrig");
    assert_eq!(
        chain_descriptions(&reloaded),
        vec![Some("g1".to_string())]
    );
}

// ────────────────────────────────────────────────────────────────────
// 6. Negative path: reload before any save must not silently swallow
// ────────────────────────────────────────────────────────────────────

#[test]
fn reload_of_missing_path_errors_instead_of_silent_empty() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("does-not-exist.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let err = load_project_session(&path, &cfg);
    assert!(err.is_err(), "missing project must surface an error");
}
