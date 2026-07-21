//! Marker/tone-block persistence tests (issue #792 split from
//! project_ops_persistence_tests.rs). Shares session fixtures via super.

use crate::project_ops::{load_project_session, save_project_session};
use crate::state::ProjectSession;
use application::command::Command;
use application::dispatcher::CommandDispatcher;
use domain::ids::{BlockId, ChainId};
use domain::value_objects::ParameterValue;
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::param::ParameterSet;
use std::path::PathBuf;
use tempfile::TempDir;
use super::project_ops_persistence_tests::{chain_descriptions, chain_for, new_session_at};

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
    assert_eq!(chain_descriptions(&reloaded), vec![Some("g1".to_string())]);
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
    assert_eq!(chain_descriptions(&reloaded), vec![Some("g1".to_string())]);
}

// ────────────────────────────────────────────────────────────────────
// 6. Negative path: reload before any save must not silently swallow
// ────────────────────────────────────────────────────────────────────

// ────────────────────────────────────────────────────────────────────
// 7. Block-level edits inside an EXISTING (rig-projected) chain
//    — the tone-shaping flow: open a saved project, tweak a block,
//    save, reopen. The suite above only covers chain-level add/remove.
// ────────────────────────────────────────────────────────────────────

fn marker_block() -> AudioBlock {
    AudioBlock {
        id: BlockId("marker:1".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "MARKER".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn has_marker(s: &ProjectSession) -> bool {
    s.project
        .borrow()
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .any(|b| matches!(&b.kind, AudioBlockKind::Core(cb) if cb.model == "MARKER"))
}

#[test]
fn editing_a_block_in_an_existing_chain_persists_on_reload() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // Seed one chain and save+reload so it becomes a `rig:`-projected chain
    // (the normal state of any reopened project — what the user edits).
    let seed = new_session_at(&path, &cfg);
    let c = chain_for(&seed, "g1");
    seed.project.borrow_mut().chains.push(c);
    save_project_session(&seed, &path).expect("seed save");

    // Reopen, add a distinctive block to the existing chain via the editor
    // path (`Command::SaveChain` upsert), then save — exactly what happens
    // when the user tweaks their rig and hits save.
    let s = load_project_session(&path, &cfg).expect("reload 1");
    let mut chain = s.project.borrow().chains[0].clone();
    let insert_at = chain.blocks.len().saturating_sub(1); // before the output block
    chain.blocks.insert(insert_at, marker_block());
    s.dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain edit");
    save_project_session(&s, &path).expect("save 2");

    // Reopen — the edit must survive.
    let reloaded = load_project_session(&path, &cfg).expect("reload 2");
    assert!(
        has_marker(&reloaded),
        "block edit on an existing chain was lost after save+reload — \
         tone-shaping edits do not persist (chains: {:?})",
        chain_descriptions(&reloaded)
    );
}

fn tone_block() -> AudioBlock {
    let mut params = ParameterSet::default();
    params.insert("gain", ParameterValue::Float(50.0));
    AudioBlock {
        id: BlockId("tone:1".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "TONEMARK".into(),
            params,
        }),
    }
}

/// Find (chain id, block id) of the TONEMARK block in the current projection.
fn find_tonemark(s: &ProjectSession) -> Option<(ChainId, BlockId)> {
    s.project.borrow().chains.iter().find_map(|c| {
        c.blocks
            .iter()
            .find(|b| matches!(&b.kind, AudioBlockKind::Core(cb) if cb.model == "TONEMARK"))
            .map(|b| (c.id.clone(), b.id.clone()))
    })
}

fn tonemark_gain(s: &ProjectSession) -> Option<f32> {
    s.project
        .borrow()
        .chains
        .iter()
        .flat_map(|c| c.blocks.iter())
        .find(|b| matches!(&b.kind, AudioBlockKind::Core(cb) if cb.model == "TONEMARK"))
        .and_then(|b| match &b.kind {
            AudioBlockKind::Core(cb) => match cb.params.get("gain") {
                Some(ParameterValue::Float(f)) => Some(*f),
                _ => None,
            },
            _ => None,
        })
}

#[test]
fn block_parameter_knob_edit_persists_on_reload() {
    // The tone-shaping flow that the user reported as "everything reverts":
    // open a saved project, turn a knob (Command::SetBlockParameterNumber),
    // save, reopen — the new value MUST survive. Unlike SaveChain, the param
    // command does not sync the edit back into the rig, so the rig-driven
    // reload restores the old value.
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // Seed: a chain carrying a TONEMARK block at gain=50, persisted via the
    // editor's SaveChain upsert (which DOES reach the rig), then reload so we
    // are editing a normal rig-projected chain.
    let seed = new_session_at(&path, &cfg);
    let c = chain_for(&seed, "g1");
    seed.project.borrow_mut().chains.push(c);
    let mut chain = seed.project.borrow().chains[0].clone();
    let at = chain.blocks.len().saturating_sub(1);
    chain.blocks.insert(at, tone_block());
    seed.dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("seed SaveChain with TONEMARK");
    save_project_session(&seed, &path).expect("seed save");

    // Reopen and confirm the block round-tripped at gain=50.
    let s = load_project_session(&path, &cfg).expect("reload 1");
    assert_eq!(
        tonemark_gain(&s),
        Some(50.0),
        "seed TONEMARK block did not round-trip"
    );

    // Turn the knob: gain 50 -> 99 via the real param command.
    let (chain_id, block_id) = find_tonemark(&s).expect("TONEMARK present after reload");
    s.dispatcher
        .dispatch(Command::SetBlockParameterNumber {
            chain: chain_id,
            block: block_id,
            path: "gain".into(),
            value: 99.0,
        })
        .expect("SetBlockParameterNumber");
    assert_eq!(
        tonemark_gain(&s),
        Some(99.0),
        "param command did not update the in-memory value"
    );
    save_project_session(&s, &path).expect("save 2");

    // Reopen — the knob change must persist.
    let reloaded = load_project_session(&path, &cfg).expect("reload 2");
    assert_eq!(
        tonemark_gain(&reloaded),
        Some(99.0),
        "REGRESSION: knob/param edit lost after save+reload — the param \
         command does not sync into the rig and the save path rebuilds from \
         the stale rig"
    );
}

fn gain_block(model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId("gain:1".into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: model.into(),
            params: ParameterSet::default(),
        }),
    }
}

/// (chain index, block index, chain id, block id) of the first gain block.
fn find_gain(s: &ProjectSession) -> Option<(usize, usize, ChainId, BlockId)> {
    let p = s.project.borrow();
    for (ci, c) in p.chains.iter().enumerate() {
        for (bi, b) in c.blocks.iter().enumerate() {
            if matches!(&b.kind, AudioBlockKind::Core(cb) if cb.effect_type == "gain") {
                return Some((ci, bi, c.id.clone(), b.id.clone()));
            }
        }
    }
    None
}

#[test]
fn replace_block_model_in_existing_chain_persists_on_reload() {
    // #627: the user's bug — swap a block's model (ReplaceBlockModel, "trocar o
    // pedal de ganho") in the active preset, save DIRECTLY (no preset/scene
    // nav), reopen. The rig write-back compared only block ids, so a same-id
    // model swap was classified as a per-scene param diff and the new model was
    // never written into the preset base → the pedal reverted on reload.
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("project.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    // Seed a chain carrying a gain block (model "volume"), persisted via the
    // editor's SaveChain upsert, then reload so it is a rig-projected chain.
    let seed = new_session_at(&path, &cfg);
    let c = chain_for(&seed, "g1");
    seed.project.borrow_mut().chains.push(c);
    let mut chain = seed.project.borrow().chains[0].clone();
    let at = chain.blocks.len().saturating_sub(1);
    chain.blocks.insert(at, gain_block("volume"));
    seed.dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("seed SaveChain with gain block");
    save_project_session(&seed, &path).expect("seed save");

    // Reopen, swap the gain block's MODEL in place (same id — exactly what
    // ReplaceBlockModel produces) and upsert via the editor's SaveChain, then
    // save directly (NO rig-nav) and reopen. Using a literal model string keeps
    // the test independent of the model registry; persistence does not validate
    // the model (unknown models load disabled but are kept — #606).
    let s = load_project_session(&path, &cfg).expect("reload 1");
    let (ci, bi, _chain_id, _block_id) = find_gain(&s).expect("gain block after reload");
    let mut chain = s.project.borrow().chains[ci].clone();
    if let AudioBlockKind::Core(cb) = &mut chain.blocks[bi].kind {
        cb.model = "swapped_pedal".into();
    }
    s.dispatcher
        .dispatch(Command::SaveChain { chain })
        .expect("SaveChain model swap");
    save_project_session(&s, &path).expect("save 2");

    let reloaded = load_project_session(&path, &cfg).expect("reload 2");
    let model = match &reloaded.project.borrow().chains[ci].blocks[bi].kind {
        AudioBlockKind::Core(cb) => cb.model.clone(),
        _ => "??".into(),
    };
    assert_eq!(
        model, "swapped_pedal",
        "block model swap was lost after a direct save+reload"
    );
}

#[test]
fn reload_of_missing_path_errors_instead_of_silent_empty() {
    let tmp = TempDir::new().unwrap();
    let path: PathBuf = tmp.path().join("does-not-exist.yaml");
    let cfg: PathBuf = tmp.path().join("config.yaml");

    let err = load_project_session(&path, &cfg);
    assert!(err.is_err(), "missing project must surface an error");
}
