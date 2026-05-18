//! Tests for the `RigProject` → synthetic `Chain` bridge (#451, T1).

use super::*;
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_graph::build_chain_runtime_state;
use domain::ids::DeviceId;
use project::block::{CoreBlock, InputEntry, OutputEntry};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::rig::{RigInput, RigOutput, RigPreset, RigProject};

const SR: f32 = 48_000.0;

fn src(dev: &str, ch: Vec<usize>) -> InputEntry {
    InputEntry {
        device_id: DeviceId(dev.into()),
        mode: ChainInputMode::Mono,
        channels: ch,
    }
}

fn out_entry(dev: &str, ch: Vec<usize>) -> OutputEntry {
    OutputEntry {
        device_id: DeviceId(dev.into()),
        mode: ChainOutputMode::Stereo,
        channels: ch,
    }
}

fn fx(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "delay".into(),
            model: "tape".into(),
            params: ParameterSet::default(),
        }),
    }
}

fn rig(
    inputs: Vec<(&str, RigInput)>,
    presets: Vec<(&str, Vec<AudioBlock>)>,
    outputs: Vec<(&str, OutputEntry)>,
) -> RigProject {
    RigProject {
        name: Some("Studio".into()),
        inputs: inputs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        outputs: outputs
            .into_iter()
            .map(|(k, e)| {
                (
                    k.to_string(),
                    RigOutput {
                        label: None,
                        entry: e,
                    },
                )
            })
            .collect(),
        presets: presets
            .into_iter()
            .map(|(k, b)| {
                (
                    k.to_string(),
                    RigPreset {
                        blocks: b,
                        scene_params: vec![],
                        scenes: std::collections::BTreeMap::new(),
                        volume: 100.0,
                    },
                )
            })
            .collect(),
    }
}

fn input(
    srcs: Vec<InputEntry>,
    bank: &[(usize, &str)],
    active: usize,
    routing: Vec<&str>,
) -> RigInput {
    RigInput {
        label: None,
        sources: srcs,
        bank: bank.iter().map(|(i, n)| (*i, n.to_string())).collect(),
        active_preset: active,
        active_scene: 1,
        routing: routing.into_iter().map(String::from).collect(),
    }
}

fn kinds(c: &Chain) -> Vec<&'static str> {
    c.blocks.iter().map(|b| b.kind.label()).collect()
}

#[test]
fn bridge_one_input_input_fx_output() {
    let r = rig(
        vec![(
            "input-1",
            input(vec![src("sc", vec![0])], &[(1, "clean")], 1, vec!["o"]),
        )],
        vec![("clean", vec![fx("d")])],
        vec![("o", out_entry("sc", vec![0, 1]))],
    );
    let chains = rig_to_chains(&r);
    assert_eq!(chains.len(), 1);
    let c = &chains[0];
    assert_eq!(c.id, ChainId("rig:input-1".into()), "deterministic id");
    assert_eq!(kinds(c), vec!["input", "core", "output"]);
    let input_blk = c.input_blocks()[0].1;
    assert_eq!(input_blk.entries, vec![src("sc", vec![0])]);
    let output_blk = c.output_blocks()[0].1;
    assert_eq!(output_blk.entries, vec![out_entry("sc", vec![0, 1])]);
}

#[test]
fn bridge_distinct_chain_ids_isolation() {
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("b", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![])],
        vec![],
    );
    let chains = rig_to_chains(&r);
    assert_eq!(chains.len(), 2);
    let ids: Vec<&str> = chains.iter().map(|c| c.id.0.as_str()).collect();
    assert_eq!(ids, vec!["rig:input-1", "rig:input-2"]);
    assert_ne!(chains[0].id, chains[1].id, "isolation: distinct runtimes");
}

#[test]
fn bridge_uses_active_preset() {
    let r = rig(
        vec![(
            "input-1",
            input(
                vec![src("sc", vec![0])],
                &[(1, "clean"), (2, "drive")],
                2,
                vec![],
            ),
        )],
        vec![
            ("clean", vec![fx("c")]),
            ("drive", vec![fx("d1"), fx("d2")]),
        ],
        vec![],
    );
    let c = &rig_to_chains(&r)[0];
    let fx_ids: Vec<&str> = c
        .blocks
        .iter()
        .filter_map(|b| match &b.kind {
            AudioBlockKind::Core(_) => Some(b.id.0.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        fx_ids,
        vec!["d1", "d2"],
        "active preset drive's blocks, in order"
    );
}

#[test]
fn rig_to_legacy_project_emits_all_inputs_enabled_flag_reflects_set() {
    use std::collections::BTreeSet;
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("b", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![fx("x")])],
        vec![],
    );

    // Empty set ⇒ every input present, ALL off (nothing auto-starts;
    // the user enables at runtime).
    let none = super::rig_to_legacy_project(&r, &BTreeSet::new());
    assert_eq!(none.chains.len(), 2, "all inputs become chains");
    assert!(
        none.chains.iter().all(|c| !c.enabled),
        "nothing is auto-enabled"
    );

    // Enabled set ⇒ same chains, only the named one flagged on.
    let some: BTreeSet<String> = ["input-2".to_string()].into_iter().collect();
    let proj = super::rig_to_legacy_project(&r, &some);
    assert_eq!(proj.chains.len(), 2);
    let c2 = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:input-2")
        .unwrap();
    let c1 = proj
        .chains
        .iter()
        .find(|c| c.id.0 == "rig:input-1")
        .unwrap();
    assert!(c2.enabled, "input-2 enabled");
    assert!(!c1.enabled, "input-1 stays off");
    assert_eq!(proj.name.as_deref(), Some("Studio"));
    assert!(proj.device_settings.is_empty());
}

#[test]
fn bridge_carries_preset_volume_not_hardcoded_100() {
    // Invariant #10: volume per stream is immutable. The synthetic chain
    // MUST carry the active preset's volume (legacy migration preserved
    // Chain.volume → RigPreset.volume); hardcoding 100 silently retunes
    // every preset on the rig path.
    let mut r = rig(
        vec![(
            "input-1",
            input(vec![src("sc", vec![0])], &[(1, "lead")], 1, vec![]),
        )],
        vec![("lead", vec![fx("a")])],
        vec![],
    );
    r.presets.get_mut("lead").unwrap().volume = 147.0;

    let c = &rig_to_chains(&r)[0];
    assert_eq!(c.volume, 147.0, "synthetic chain must carry preset volume");
}

#[test]
fn bridge_preserves_multi_source() {
    let r = rig(
        vec![(
            "input-1",
            input(
                vec![src("sc", vec![0]), src("sc", vec![1])],
                &[(1, "p")],
                1,
                vec![],
            ),
        )],
        vec![("p", vec![])],
        vec![],
    );
    let c = &rig_to_chains(&r)[0];
    assert_eq!(c.input_blocks()[0].1.entries.len(), 2);
}

#[test]
fn bridge_empty_routing_no_output_block() {
    let r = rig(
        vec![(
            "input-1",
            input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
        )],
        vec![("p", vec![])],
        vec![],
    );
    let c = &rig_to_chains(&r)[0];
    assert!(c.output_blocks().is_empty());
}

// ── T2/T3: RigRuntime controller + lock-free preset swap ──────────────────

use crate::runtime_state::ChainRuntimeState;
use std::sync::Arc;

fn arc_for<'a>(rt: &'a RigRuntime, input: &str) -> &'a Arc<ChainRuntimeState> {
    rt.graph()
        .chains
        .get(&(ChainId(format!("rig:{input}")), 0))
        .expect("runtime for input")
}

#[test]
fn runtime_builds_n_isolated_runtimes() {
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("b", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![])],
        vec![],
    );
    let rt = RigRuntime::build(r, SR).expect("build");
    assert_eq!(rt.graph().chains.len(), 2, "one isolated runtime per input");
    assert!(!Arc::ptr_eq(
        arc_for(&rt, "input-1"),
        arc_for(&rt, "input-2")
    ));
}

#[test]
fn switch_preset_updates_active_index() {
    let r = rig(
        vec![(
            "input-1",
            input(
                vec![src("a", vec![0])],
                &[(1, "clean"), (2, "drive")],
                1,
                vec![],
            ),
        )],
        vec![("clean", vec![]), ("drive", vec![])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    rt.switch_preset("input-1", 2).expect("switch ok");
    assert_eq!(rt.project().inputs["input-1"].active_preset, 2);
}

#[test]
fn switch_preset_invalid_index_errs_and_keeps_active() {
    let r = rig(
        vec![(
            "input-1",
            input(vec![src("a", vec![0])], &[(1, "clean")], 1, vec![]),
        )],
        vec![("clean", vec![])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    assert!(rt.switch_preset("input-1", 9).is_err());
    assert_eq!(rt.project().inputs["input-1"].active_preset, 1);
}

#[test]
fn switch_preset_does_not_touch_other_input_isolation() {
    let r = rig(
        vec![
            (
                "input-1",
                input(
                    vec![src("a", vec![0])],
                    &[(1, "clean"), (2, "drive")],
                    1,
                    vec![],
                ),
            ),
            (
                "input-2",
                input(vec![src("b", vec![0])], &[(1, "clean")], 1, vec![]),
            ),
        ],
        vec![("clean", vec![]), ("drive", vec![])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    let other_before = Arc::clone(arc_for(&rt, "input-2"));
    rt.switch_preset("input-1", 2).expect("switch");
    assert!(
        Arc::ptr_eq(&other_before, arc_for(&rt, "input-2")),
        "switching input-1 must not rebuild input-2 (invariant #4)"
    );
}

#[test]
fn switch_preset_is_lockfree_inplace_same_runtime_arc() {
    // Same I/O signature, only processing blocks differ ⇒ the proven in-place
    // update path keeps the SAME Arc<ChainRuntimeState> (lock-free swap, not
    // teardown). If this regresses to a full rebuild the Arc pointer changes.
    let r = rig(
        vec![(
            "input-1",
            input(
                vec![src("a", vec![0])],
                &[(1, "clean"), (2, "drive")],
                1,
                vec!["o"],
            ),
        )],
        vec![("clean", vec![]), ("drive", vec![])],
        vec![("o", out_entry("a", vec![0, 1]))],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    let before = Arc::clone(arc_for(&rt, "input-1"));
    rt.switch_preset("input-1", 2).expect("switch");
    assert!(
        Arc::ptr_eq(&before, arc_for(&rt, "input-1")),
        "preset swap must reuse the runtime Arc (lock-free in-place)"
    );
}

#[test]
fn bridge_result_builds_runtime() {
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec!["o"]),
            ),
            (
                "input-2",
                input(vec![src("sc", vec![1])], &[(1, "p")], 1, vec!["o"]),
            ),
        ],
        vec![("p", vec![])],
        vec![("o", out_entry("sc", vec![0, 1]))],
    );
    for c in rig_to_chains(&r) {
        build_chain_runtime_state(&c, SR, &[DEFAULT_ELASTIC_TARGET])
            .unwrap_or_else(|e| panic!("synthetic chain {} must build: {e}", c.id.0));
    }
}

// ── #454 T4: scene-aware bridge + RigRuntime::switch_scene ─────────────────

use project::rig::RigScene;
use std::collections::BTreeMap;

fn rig_with_scene() -> RigProject {
    let mut r = rig(
        vec![(
            "input-1",
            input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
        )],
        vec![("p", vec![fx("od")])],
        vec![],
    );
    let preset = r.presets.get_mut("p").unwrap();
    preset.scenes = BTreeMap::from([(
        2,
        RigScene {
            label: None,
            bypass: BTreeMap::from([("od".to_string(), true)]),
            params: BTreeMap::new(),
        },
    )]);
    r
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
            (
                "input-1",
                input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("b", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![fx("od")])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
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
    let mut rt = RigRuntime::build(rig_with_scene(), SR).expect("build");
    assert!(rt.switch_scene("input-1", 9).is_err());
    assert!(rt.switch_scene("input-1", 0).is_err());
    assert_eq!(rt.project().inputs["input-1"].active_scene, 1);
}

#[test]
fn switch_scene_is_lockfree_same_runtime_arc() {
    let r = rig(
        vec![(
            "input-1",
            input(vec![src("a", vec![0])], &[(1, "p")], 1, vec!["o"]),
        )],
        vec![("p", vec![fx("od")])],
        vec![("o", out_entry("a", vec![0, 1]))],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
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
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![])],
        vec![],
    );
    let rt = RigRuntime::build(r, SR).expect("build");
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
            (
                "input-1",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    let err = rt.enable_input("input-2").unwrap_err().to_string();
    assert!(err.contains("sc") && err.contains("input-1"), "got: {err}");
    assert!(!rt.is_enabled("input-2"));
}

#[test]
fn disable_then_enable_other_frees_the_tap() {
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    rt.disable_input("input-1").expect("disable");
    assert!(!rt.is_enabled("input-1"));
    rt.enable_input("input-2").expect("tap now free");
    assert!(rt.is_enabled("input-2"));
    assert_eq!(rt.graph().chains.len(), 1);
}

#[test]
fn enable_disable_unknown_input_errs() {
    let r = rig(
        vec![(
            "input-1",
            input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
        )],
        vec![("p", vec![])],
        vec![],
    );
    let mut rt = RigRuntime::build(r, SR).expect("build");
    assert!(rt.enable_input("ghost").is_err());
    assert!(rt.disable_input("ghost").is_err());
}

#[test]
fn non_conflicting_inputs_both_auto_enabled() {
    let r = rig(
        vec![
            (
                "input-1",
                input(vec![src("sc", vec![0])], &[(1, "p")], 1, vec![]),
            ),
            (
                "input-2",
                input(vec![src("sc", vec![1])], &[(1, "p")], 1, vec![]),
            ),
        ],
        vec![("p", vec![])],
        vec![],
    );
    let rt = RigRuntime::build(r, SR).expect("build");
    assert!(rt.is_enabled("input-1") && rt.is_enabled("input-2"));
    assert_eq!(rt.graph().chains.len(), 2);
}

// #436 #1: pure preset/scene switch + reproject (drives the legacy GUI path)

#[test]
fn switch_and_project_changes_active_preset_and_rebuilds_chain() {
    let mut r = rig(
        vec![(
            "input-1",
            input(
                vec![src("a", vec![0])],
                &[(1, "clean"), (2, "drive")],
                1,
                vec![],
            ),
        )],
        vec![("clean", vec![fx("c")]), ("drive", vec![fx("d")])],
        vec![],
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
        vec![(
            "input-1",
            input(vec![src("a", vec![0])], &[(1, "p")], 1, vec![]),
        )],
        vec![("p", vec![fx("x")])],
        vec![],
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
