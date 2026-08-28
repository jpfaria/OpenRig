//! #913 — the decisions the looper callbacks make before dispatching.
//!
//! Two of them matter to the player. The speed control is a three-way pick
//! whose middle position must be normal speed — a wrong mapping plays every
//! loop at the wrong pitch. And #323 phase 2: a FRESH recording is linked to
//! the chain's active preset so it keeps that tone after the chain switches
//! preset to solo, while an OVERDUB never relinks — relinking there would
//! retag a loop the player built under a different tone.

use super::{chain_id_at, link_active_preset_on_fresh_record, speed_from_index};
use crate::state::ProjectSession;
use application::live_source::LiveSource;
use domain::ids::ChainId;
use engine::LooperSpeed;
use project::chain::Chain;
use project::project::Project;
use project::rig::{RigInput, RigPreset, RigProject};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

// ── Speed ─────────────────────────────────────────────────────────────────

#[test]
fn the_middle_speed_position_is_normal() {
    assert_eq!(speed_from_index(1), LooperSpeed::Normal);
}

#[test]
fn the_outer_speed_positions_are_half_and_double() {
    assert_eq!(speed_from_index(0), LooperSpeed::Half);
    assert_eq!(speed_from_index(2), LooperSpeed::Double);
}

#[test]
fn an_index_outside_the_control_falls_back_to_normal_speed() {
    // A stale index must never leave the loop playing at the wrong pitch.
    assert_eq!(speed_from_index(-1), LooperSpeed::Normal);
    assert_eq!(speed_from_index(7), LooperSpeed::Normal);
}

// ── Which chain a row is ──────────────────────────────────────────────────

fn chain(id: &str) -> Chain {
    Chain {
        id: ChainId(id.into()),
        description: None,
        instrument: "electric_guitar".into(),
        enabled: false,
        volume: 100.0,
        io_binding_ids: vec![],
        blocks: vec![],
        di_output: None,
        loopers: vec![],
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
        std::env::temp_dir().join("openrig-913-looper-tests"),
    );
    session.rig = rig.map(|r| Rc::new(RefCell::new(r)));
    session
}

#[test]
fn a_row_resolves_to_its_own_chain() {
    let session = session(vec![chain("chain:0"), chain("chain:1")], None);
    assert_eq!(chain_id_at(&session, 1), Some(ChainId("chain:1".into())));
}

#[test]
fn a_stale_row_resolves_to_no_chain() {
    let session = session(vec![chain("chain:0")], None);
    assert_eq!(chain_id_at(&session, 4), None);
    assert_eq!(chain_id_at(&session, -1), None);
}

// ── Linking a fresh recording to the active preset ────────────────────────

/// A rig whose only input is playing the preset named here.
fn rig_playing(preset_id: &str) -> RigProject {
    RigProject {
        name: None,
        inputs: BTreeMap::from([(
            "guitar".to_string(),
            RigInput {
                label: None,
                bank: BTreeMap::from([(1, preset_id.to_string())]),
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
            preset_id.to_string(),
            RigPreset {
                id: preset_id.to_string(),
                name: Some("Lead".to_string()),
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

/// A frontend hosting no looper runtime: the rig is stopped, so there is no
/// reading at all — which is the state a fresh record starts from.
struct NoLoopers;
impl LiveSource for NoLoopers {}

fn linked_preset(session: &ProjectSession, chain: &ChainId, uid: u64) -> Option<String> {
    let project = session.project.borrow();
    project
        .chains
        .iter()
        .find(|c| &c.id == chain)?
        .loopers
        .iter()
        .find(|l| l.uid == uid)?
        .preset
        .clone()
}

#[test]
fn a_chain_that_is_not_projected_from_a_rig_links_nothing() {
    let session = session(vec![chain("chain:0")], None);
    let live: Rc<dyn LiveSource> = Rc::new(NoLoopers);
    link_active_preset_on_fresh_record(&session, &live, &ChainId("chain:0".into()), 1);
    assert_eq!(linked_preset(&session, &ChainId("chain:0".into()), 1), None);
}

#[test]
fn a_fresh_recording_on_a_rig_chain_is_linked_to_the_active_preset() {
    // #323 phase 2: the loop keeps the tone it was recorded under, even after
    // the chain switches preset to solo.
    let session = session(
        vec![chain("rig:guitar")],
        Some(rig_playing("lead-boost")),
    );
    session.project.borrow_mut().chains[0]
        .loopers
        .push(project::looper::LooperConfig::new(1));
    let live: Rc<dyn LiveSource> = Rc::new(NoLoopers);

    link_active_preset_on_fresh_record(&session, &live, &ChainId("rig:guitar".into()), 1);

    assert_eq!(
        linked_preset(&session, &ChainId("rig:guitar".into()), 1).as_deref(),
        Some("lead-boost")
    );
}
