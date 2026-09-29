//! #328 — a split is PRESET content: switching preset or scene must bring the
//! new preset's split (its paths, its knobs, its scene values), never keep the
//! one the chain had. The port merge used to treat every `is_routing()` block
//! as a chain port, and a split is routing.

use std::collections::BTreeMap;

use project::block::SplitEnd;
use project::rig::RigScene;

use crate::command::RigNavKind;
use crate::local_dispatcher_tests::*;
use crate::split_tests_fixtures::*;

#[test]
fn switching_preset_puts_the_new_presets_split_in_place_of_the_old_one() {
    let (_rig, project, dispatcher) = rig_session_from(rig_with_presets(vec![
        (
            "p1",
            vec![
                make_core_block("A", true),
                split(
                    "S1",
                    SplitEnd::Mix,
                    vec![make_core_block("X", true)],
                    vec![],
                ),
            ],
        ),
        (
            "p2",
            vec![
                make_core_block("B", true),
                split(
                    "S2",
                    SplitEnd::Mix,
                    vec![make_core_block("Y", true)],
                    vec![],
                ),
            ],
        ),
    ]));

    dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: ChainId(RIG_CHAIN.into()),
            // Preset position 1 → the 2nd bank key → p2.
            kind: RigNavKind::Preset(1),
        }))
        .expect("switch to p2");

    let project = project.borrow();
    assert_eq!(
        ids(&project.chains[0].blocks),
        vec!["B", "S2"],
        "p2's own split, not p1's"
    );
    assert_eq!(ids(&split_of(&project).a), vec!["Y"]);
}

#[test]
fn switching_scene_applies_the_scene_bypass_to_a_block_inside_a_path() {
    let mut rig = rig_with_presets(vec![(
        "p1",
        vec![
            make_core_block("A", true),
            split(
                "S1",
                SplitEnd::Mix,
                vec![make_core_block("X", true)],
                vec![],
            ),
        ],
    )]);
    let preset = rig.presets.get_mut("p1").expect("p1");
    preset.scenes.insert(1, RigScene::default());
    preset.scenes.insert(
        2,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("X".to_string(), true)]),
            params: BTreeMap::new(),
            volume: None,
        },
    );
    let (_rig, project, dispatcher) = rig_session_from(rig);

    dispatcher
        .dispatch(Command::Selection(SelectionCommand::ApplyRigNav {
            chain: ChainId(RIG_CHAIN.into()),
            kind: RigNavKind::Scene(2),
        }))
        .expect("switch to scene 2");

    assert!(
        !split_of(&project.borrow()).a[0].enabled,
        "scene 2 bypasses X inside path A"
    );
}
