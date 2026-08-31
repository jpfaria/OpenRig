//! #913 — saving a chain as a named preset.
//!
//! Three decisions live here and each has a user-visible failure behind it:
//! naming the file after the TONE rather than the chain (#518), treating an
//! emptied field as "keep the default" rather than saving under no name, and
//! renaming the active preset so the chain-title combobox does not keep showing
//! the old label as if nothing had happened (#510). The chain itself is read
//! from the session at commit time — the dispatcher owns the write since #555.

use super::{chosen_name, commit_preset_save, pending_save_for, PresetSaveError};
use crate::state::ProjectSession;
use domain::ids::{BlockId, ChainId};
use project::block::{AudioBlock, AudioBlockKind, CoreBlock};
use project::chain::Chain;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

fn block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".into(),
            model: "volume".into(),
            params: Default::default(),
        }),
    }
}

fn chain(id: &str, description: Option<&str>) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: description.map(str::to_string),
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![block("gain")],
        di_output: None,
        loopers: vec![],
    }
}

fn rig_with_active_preset(input: &str, preset_name: &str) -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([(
            input.to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, "the-preset".to_string())]),
                active_preset: 1,
                active_scene: 1,
                routing: Vec::new(),
                instrument: "electric_guitar".to_string(),
                io: String::new(),
                endpoint: String::new(),
                io_binding_ids: Vec::new(),
                loopers: Vec::new(),
            },
        )]),
        outputs: BTreeMap::new(),
        presets: BTreeMap::from([(
            "the-preset".to_string(),
            RigPreset {
                id: "the-preset".to_string(),
                name: Some(preset_name.to_string()),
                blocks: Vec::new(),
                scene_params: Vec::new(),
                scenes: BTreeMap::new(),
                volume: 100.0,
            },
        )]),
        midi: None,
        chain_order: Vec::new(),
    }
}

fn session(chains: Vec<Chain>, rig: Option<RigProject>) -> ProjectSession {
    let mut session = ProjectSession::new(
        Project {
            name: None,
            device_settings: vec![],
            chains,
            midi: None,
        },
        None,
        None,
        std::env::temp_dir().join("openrig-913-preset-save-tests"),
    );
    session.rig = rig.map(|r| Rc::new(RefCell::new(r)));
    session
}

#[test]
fn the_default_name_is_the_active_presets_name_not_the_chain_title() {
    let session = session(
        vec![chain("rig:guitar", Some("Guitar"))],
        Some(rig_with_active_preset("guitar", "Lead Boost")),
    );
    let pending = pending_save_for(&session, 0).expect("chain 0 exists");
    assert_eq!(
        pending.default_name, "Lead Boost",
        "#518: the file is named after the tone, not the chain"
    );
}

#[test]
fn a_chain_that_is_not_projected_from_a_rig_falls_back_to_its_description() {
    let session = session(vec![chain("chain:0", Some("Guitar"))], None);
    let pending = pending_save_for(&session, 0).expect("chain 0 exists");
    assert_eq!(pending.default_name, "Guitar");
}

#[test]
fn a_chain_with_no_description_falls_back_to_its_position() {
    let session = session(vec![chain("chain:0", None), chain("chain:1", None)], None);
    assert_eq!(
        pending_save_for(&session, 1).expect("chain 1").default_name,
        "chain_2",
        "one-based, so it matches what the row shows"
    );
}

#[test]
fn a_row_index_with_no_chain_behind_it_cannot_start_a_save() {
    let session = session(vec![chain("chain:0", None)], None);
    assert!(matches!(
        pending_save_for(&session, 9),
        Err(PresetSaveError::NoSuchChain)
    ));
}

#[test]
fn an_emptied_field_keeps_the_default_name() {
    assert_eq!(chosen_name("", "Lead Boost"), "Lead Boost");
    assert_eq!(chosen_name("   ", "Lead Boost"), "Lead Boost");
}

#[test]
fn a_typed_name_is_used_with_its_surrounding_spaces_trimmed() {
    assert_eq!(chosen_name("  Rhythm  ", "Lead Boost"), "Rhythm");
}

#[test]
fn the_users_own_punctuation_survives() {
    assert_eq!(
        chosen_name("Lead Boost!", "x"),
        "Lead Boost!",
        "#510: the name is passed through verbatim, never slugged"
    );
}

#[test]
fn committing_a_save_is_accepted_by_the_bus() {
    let session = session(vec![chain("chain:0", Some("Guitar"))], None);
    let pending = pending_save_for(&session, 0).expect("chain 0");
    assert_eq!(
        commit_preset_save(&session, &pending.chain_id, "Rhythm"),
        Ok(())
    );
}

#[test]
fn committing_for_a_chain_that_is_gone_reports_the_failure() {
    let session = session(vec![chain("chain:0", None)], None);
    let error = commit_preset_save(&session, &ChainId("chain:gone".into()), "Rhythm")
        .expect_err("a chain that is not in the project cannot be saved");
    assert!(!error.is_empty());
}
