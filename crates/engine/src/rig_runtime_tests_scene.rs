//! Rig scene tests (issue #792 split from rig_runtime_tests.rs).
//! Shares the binding/rig fixtures with the base suite via super::tests.

use std::sync::Arc;

use super::tests::{arc_for, binding, fx, in_ep, input, out_ep, rig, SR};
use super::*;

// ── #454 T4: scene-aware bridge + RigRuntime::switch_scene ─────────────────

use project::rig::RigScene;
use std::collections::BTreeMap;

fn rig_with_scene() -> RigProject {
    let mut r = rig(
        vec![("input-1", input("io_a", &[(1, "p")], 1))],
        vec![("p", vec![fx("od")])],
    );
    let preset = r.presets.get_mut("p").unwrap();
    preset.scenes = BTreeMap::from([(
        2,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("od".to_string(), true)]),
            params: BTreeMap::new(),
            volume: None,
        },
    )]);
    r
}

fn scene_registry() -> Vec<IoBinding> {
    vec![binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![])]
}

#[test]
fn rig_to_chains_applies_active_scene_bypass() {
    let mut r = rig_with_scene();
    r.inputs.get_mut("input-1").unwrap().active_scene = 2;
    let c = &rig_to_chains(&r)[0];
    let od = c.blocks.iter().find(|b| b.id.0 == "od").unwrap();
    assert!(!od.enabled, "scene 2 bypasses od");
}

#[test]
fn switch_scene_updates_active_and_rebuilds_only_that_input() {
    let r = rig(
        vec![
            ("input-1", input("io_a", &[(1, "p")], 1)),
            ("input-2", input("io_b", &[(1, "p")], 1)),
        ],
        vec![("p", vec![fx("od")])],
    );
    let registry = vec![
        binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![]),
        binding("io_b", vec![in_ep("in0", "b", vec![0])], vec![]),
    ];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    let other_before = Arc::clone(arc_for(&rt, "input-2"));
    rt.switch_scene("input-1", 3).expect("switch scene");
    assert_eq!(rt.project().inputs["input-1"].active_scene, 3);
    assert!(
        Arc::ptr_eq(&other_before, arc_for(&rt, "input-2")),
        "switching scene on input-1 must not touch input-2 (invariant #4)"
    );
}

#[test]
fn switch_scene_invalid_index_errs_and_keeps_active() {
    let mut rt = RigRuntime::build(rig_with_scene(), SR, scene_registry()).expect("build");
    assert!(rt.switch_scene("input-1", 9).is_err());
    assert!(rt.switch_scene("input-1", 0).is_err());
    assert_eq!(rt.project().inputs["input-1"].active_scene, 1);
}

#[test]
fn switch_scene_is_lockfree_same_runtime_arc() {
    let r = rig(
        vec![("input-1", input("io_a", &[(1, "p")], 1))],
        vec![("p", vec![fx("od")])],
    );
    let registry = vec![binding(
        "io_a",
        vec![in_ep("in0", "a", vec![0])],
        vec![out_ep("out0", "a", vec![0, 1])],
    )];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    let before = Arc::clone(arc_for(&rt, "input-1"));
    rt.switch_scene("input-1", 2).expect("switch");
    assert!(
        Arc::ptr_eq(&before, arc_for(&rt, "input-1")),
        "scene swap reuses the runtime Arc (lock-free in-place, like #451)"
    );
}

// ── runtime tap-exclusivity (enabled is in-memory only) ───────────────────

#[test]
fn build_skips_second_input_conflicting_on_same_tap() {
    // Both inputs select bindings that tap the same (device, channel).
    let r = rig(
        vec![
            ("input-1", input("io1", &[(1, "p")], 1)),
            ("input-2", input("io2", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
    );
    let registry = vec![
        binding("io1", vec![in_ep("in0", "sc", vec![0])], vec![]),
        binding("io2", vec![in_ep("in0", "sc", vec![0])], vec![]),
    ];
    let rt = RigRuntime::build(r, SR, registry).expect("build");
    assert_eq!(
        rt.graph().chains.len(),
        1,
        "conflicting input not auto-enabled"
    );
    assert!(rt.is_enabled("input-1"));
    assert!(!rt.is_enabled("input-2"));
}

#[test]
fn enable_input_rejects_tap_already_in_use() {
    let r = rig(
        vec![
            ("input-1", input("io1", &[(1, "p")], 1)),
            ("input-2", input("io2", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
    );
    let registry = vec![
        binding("io1", vec![in_ep("in0", "sc", vec![0])], vec![]),
        binding("io2", vec![in_ep("in0", "sc", vec![0])], vec![]),
    ];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    let err = rt.enable_input("input-2").unwrap_err().to_string();
    assert!(err.contains("sc") && err.contains("input-1"), "got: {err}");
    assert!(!rt.is_enabled("input-2"));
}

#[test]
fn disable_then_enable_other_frees_the_tap() {
    let r = rig(
        vec![
            ("input-1", input("io1", &[(1, "p")], 1)),
            ("input-2", input("io2", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
    );
    let registry = vec![
        binding("io1", vec![in_ep("in0", "sc", vec![0])], vec![]),
        binding("io2", vec![in_ep("in0", "sc", vec![0])], vec![]),
    ];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    rt.disable_input("input-1").expect("disable");
    assert!(!rt.is_enabled("input-1"));
    rt.enable_input("input-2").expect("tap now free");
    assert!(rt.is_enabled("input-2"));
    assert_eq!(rt.graph().chains.len(), 1);
}

#[test]
fn enable_disable_unknown_input_errs() {
    let r = rig(
        vec![("input-1", input("io_a", &[(1, "p")], 1))],
        vec![("p", vec![])],
    );
    let registry = vec![binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![])];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    assert!(rt.enable_input("ghost").is_err());
    assert!(rt.disable_input("ghost").is_err());
}

#[test]
fn non_conflicting_inputs_both_auto_enabled() {
    let r = rig(
        vec![
            ("input-1", input("io1", &[(1, "p")], 1)),
            ("input-2", input("io2", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
    );
    let registry = vec![
        binding("io1", vec![in_ep("in0", "sc", vec![0])], vec![]),
        binding("io2", vec![in_ep("in0", "sc", vec![1])], vec![]),
    ];
    let rt = RigRuntime::build(r, SR, registry).expect("build");
    assert!(rt.is_enabled("input-1") && rt.is_enabled("input-2"));
    assert_eq!(rt.graph().chains.len(), 2);
}

// #436 #1: pure preset/scene switch + reproject (drives the legacy GUI path)

#[test]
fn switch_and_project_changes_active_preset_and_rebuilds_chain() {
    let mut r = rig(
        vec![("input-1", input("io_a", &[(1, "clean"), (2, "drive")], 1))],
        vec![("clean", vec![fx("c")]), ("drive", vec![fx("d")])],
    );

    let chain =
        super::switch_and_project_input(&mut r, "input-1", Some(2), None).expect("buildable chain");
    assert_eq!(chain.id.0, "rig:input-1");
    assert_eq!(r.inputs["input-1"].active_preset, 2, "active preset moved");
    let fx_ids: Vec<&str> = chain
        .blocks
        .iter()
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Core(_) => Some(b.id.0.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(fx_ids, vec!["d"], "rebuilt with the new preset's blocks");
}

#[test]
fn switch_and_project_sets_scene_in_range_only() {
    let mut r = rig(
        vec![("input-1", input("io_a", &[(1, "p")], 1))],
        vec![("p", vec![fx("x")])],
    );
    assert!(super::switch_and_project_input(&mut r, "input-1", None, Some(5)).is_some());
    assert_eq!(r.inputs["input-1"].active_scene, 5);

    // Out of range ⇒ rejected, no mutation.
    assert!(super::switch_and_project_input(&mut r, "input-1", None, Some(9)).is_none());
    assert_eq!(
        r.inputs["input-1"].active_scene, 5,
        "unchanged on bad scene"
    );
    // Unknown input ⇒ None.
    assert!(super::switch_and_project_input(&mut r, "ghost", Some(1), None).is_none());
    // Bad bank slot ⇒ None, no mutation.
    assert!(super::switch_and_project_input(&mut r, "input-1", Some(7), None).is_none());
    assert_eq!(r.inputs["input-1"].active_preset, 1);
}
