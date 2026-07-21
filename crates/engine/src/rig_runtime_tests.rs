//! Tests for the `RigProject` → synthetic `Chain` bridge (#451, T1).

use super::*;
use crate::runtime_audio_frame::DEFAULT_ELASTIC_TARGET;
use crate::runtime_endpoints::resolve_chain_io;
use crate::runtime_graph::build_chain_runtime_state;
use domain::ids::DeviceId;
use domain::io_binding::{ChannelMode, IoBinding, IoEndpoint};
use project::block::CoreBlock;
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};

pub(super) const SR: f32 = 48_000.0;

/// One mono input endpoint (was the old per-input `InputEntry`).
pub(super) fn in_ep(name: &str, dev: &str, ch: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Mono,
        channels: ch,
    }
}

/// One stereo output endpoint (was the old per-input `OutputEntry`).
pub(super) fn out_ep(name: &str, dev: &str, ch: Vec<usize>) -> IoEndpoint {
    IoEndpoint {
        name: name.into(),
        device_id: DeviceId(dev.into()),
        mode: ChannelMode::Stereo,
        channels: ch,
    }
}

/// Build one registry binding (`id`) mirroring a chain's old head inputs and
/// tail outputs. The chain selects it via `io_binding_ids`.
pub(super) fn binding(id: &str, inputs: Vec<IoEndpoint>, outputs: Vec<IoEndpoint>) -> IoBinding {
    IoBinding {
        id: id.into(),
        name: id.to_uppercase(),
        inputs,
        outputs,
    }
}

pub(super) fn fx(id: &str) -> AudioBlock {
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

pub(super) fn rig(inputs: Vec<(&str, RigInput)>, presets: Vec<(&str, Vec<AudioBlock>)>) -> RigProject {
    RigProject {
        name: Some("Studio".into()),
        inputs: inputs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        outputs: Default::default(),
        presets: presets
            .into_iter()
            .map(|(k, b)| (k.to_string(), RigPreset::from_legacy_blocks(b, 100.0)))
            .collect(),
        midi: None,
        chain_order: Vec::new(),
    }
}

/// A binding-bound input (#716): its I/O is discovered from the selected
/// registry binding, never embedded as device blocks.
pub(super) fn input(binding_id: &str, bank: &[(usize, &str)], active: usize) -> RigInput {
    RigInput {
        label: None,
        bank: bank.iter().map(|(i, n)| (*i, n.to_string())).collect(),
        active_preset: active,
        active_scene: 1,
        routing: Vec::new(),
        instrument: block_core::DEFAULT_INSTRUMENT.to_string(),
        io: String::new(),
        endpoint: String::new(),
        io_binding_ids: vec![binding_id.to_string()],
    }
}

fn kinds(c: &Chain) -> Vec<&'static str> {
    c.blocks.iter().map(|b| b.kind.label()).collect()
}

#[test]
fn bridge_one_input_input_fx_output() {
    let r = rig(
        vec![("input-1", input("io1", &[(1, "clean")], 1))],
        vec![("clean", vec![fx("d")])],
    );
    let registry = vec![binding(
        "io1",
        vec![in_ep("in0", "sc", vec![0])],
        vec![out_ep("out0", "sc", vec![0, 1])],
    )];
    let chains = rig_to_chains(&r);
    assert_eq!(chains.len(), 1);
    let c = &chains[0];
    assert_eq!(c.id, ChainId("rig:input-1".into()), "deterministic id");
    // #716: a binding-bound chain carries ONLY processing blocks; its I/O is
    // discovered from the registry, not synthesized as device blocks.
    assert_eq!(kinds(c), vec!["core"]);
    assert_eq!(c.io_binding_ids, vec!["io1".to_string()]);
    let (inputs, outputs) = resolve_chain_io(c, &registry);
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].device_id, DeviceId("sc".into()));
    assert_eq!(inputs[0].channels, vec![0]);
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].device_id, DeviceId("sc".into()));
    assert_eq!(outputs[0].channels, vec![0, 1]);
}

#[test]
fn bridge_distinct_chain_ids_isolation() {
    let r = rig(
        vec![
            ("input-1", input("io_a", &[(1, "p")], 1)),
            ("input-2", input("io_b", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
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
        vec![("input-1", input("io1", &[(1, "clean"), (2, "drive")], 2))],
        vec![
            ("clean", vec![fx("c")]),
            ("drive", vec![fx("d1"), fx("d2")]),
        ],
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
            ("input-1", input("io_a", &[(1, "p")], 1)),
            ("input-2", input("io_b", &[(1, "p")], 1)),
        ],
        vec![("p", vec![fx("x")])],
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
        vec![("input-1", input("io1", &[(1, "lead")], 1))],
        vec![("lead", vec![fx("a")])],
    );
    r.presets.get_mut("lead").unwrap().volume = 147.0;

    let c = &rig_to_chains(&r)[0];
    assert_eq!(c.volume, 147.0, "synthetic chain must carry preset volume");
}

#[test]
fn bridge_preserves_multi_source() {
    // Two mono input endpoints in the selected binding ⇒ two resolved inputs.
    let r = rig(
        vec![("input-1", input("io1", &[(1, "p")], 1))],
        vec![("p", vec![])],
    );
    let registry = vec![binding(
        "io1",
        vec![in_ep("in0", "sc", vec![0]), in_ep("in1", "sc", vec![1])],
        vec![],
    )];
    let c = &rig_to_chains(&r)[0];
    let (inputs, _) = resolve_chain_io(c, &registry);
    assert_eq!(inputs.len(), 2);
}

#[test]
fn bridge_empty_routing_no_output_block() {
    // A binding with no output endpoints ⇒ the chain resolves to no outputs.
    let r = rig(
        vec![("input-1", input("io1", &[(1, "p")], 1))],
        vec![("p", vec![])],
    );
    let registry = vec![binding("io1", vec![in_ep("in0", "sc", vec![0])], vec![])];
    let c = &rig_to_chains(&r)[0];
    assert!(c.output_blocks().is_empty());
    let (_, outputs) = resolve_chain_io(c, &registry);
    assert!(outputs.is_empty());
}

// ── T2/T3: RigRuntime controller + lock-free preset swap ──────────────────

use crate::runtime_state::ChainRuntimeState;
use std::sync::Arc;

pub(super) fn arc_for<'a>(rt: &'a RigRuntime, input: &str) -> &'a Arc<ChainRuntimeState> {
    rt.graph()
        .chains
        .get(&(ChainId(format!("rig:{input}")), 0))
        .expect("runtime for input")
}

#[test]
fn runtime_builds_n_isolated_runtimes() {
    let r = rig(
        vec![
            ("input-1", input("io_a", &[(1, "p")], 1)),
            ("input-2", input("io_b", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
    );
    let registry = vec![
        binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![]),
        binding("io_b", vec![in_ep("in0", "b", vec![0])], vec![]),
    ];
    let rt = RigRuntime::build(r, SR, registry).expect("build");
    assert_eq!(rt.graph().chains.len(), 2, "one isolated runtime per input");
    assert!(!Arc::ptr_eq(
        arc_for(&rt, "input-1"),
        arc_for(&rt, "input-2")
    ));
}

#[test]
fn switch_preset_updates_active_index() {
    let r = rig(
        vec![("input-1", input("io_a", &[(1, "clean"), (2, "drive")], 1))],
        vec![("clean", vec![]), ("drive", vec![])],
    );
    let registry = vec![binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![])];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    rt.switch_preset("input-1", 2).expect("switch ok");
    assert_eq!(rt.project().inputs["input-1"].active_preset, 2);
}

#[test]
fn switch_preset_invalid_index_errs_and_keeps_active() {
    let r = rig(
        vec![("input-1", input("io_a", &[(1, "clean")], 1))],
        vec![("clean", vec![])],
    );
    let registry = vec![binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![])];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
    assert!(rt.switch_preset("input-1", 9).is_err());
    assert_eq!(rt.project().inputs["input-1"].active_preset, 1);
}

#[test]
fn switch_preset_does_not_touch_other_input_isolation() {
    let r = rig(
        vec![
            ("input-1", input("io_a", &[(1, "clean"), (2, "drive")], 1)),
            ("input-2", input("io_b", &[(1, "clean")], 1)),
        ],
        vec![("clean", vec![]), ("drive", vec![])],
    );
    let registry = vec![
        binding("io_a", vec![in_ep("in0", "a", vec![0])], vec![]),
        binding("io_b", vec![in_ep("in0", "b", vec![0])], vec![]),
    ];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
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
        vec![("input-1", input("io_a", &[(1, "clean"), (2, "drive")], 1))],
        vec![("clean", vec![]), ("drive", vec![])],
    );
    let registry = vec![binding(
        "io_a",
        vec![in_ep("in0", "a", vec![0])],
        vec![out_ep("out0", "a", vec![0, 1])],
    )];
    let mut rt = RigRuntime::build(r, SR, registry).expect("build");
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
            ("input-1", input("io_0", &[(1, "p")], 1)),
            ("input-2", input("io_1", &[(1, "p")], 1)),
        ],
        vec![("p", vec![])],
    );
    let registry = vec![
        binding(
            "io_0",
            vec![in_ep("in0", "sc", vec![0])],
            vec![out_ep("out0", "sc", vec![0, 1])],
        ),
        binding(
            "io_1",
            vec![in_ep("in0", "sc", vec![1])],
            vec![out_ep("out0", "sc", vec![0, 1])],
        ),
    ];
    for c in rig_to_chains(&r) {
        build_chain_runtime_state(&c, SR, &[DEFAULT_ELASTIC_TARGET], &registry)
            .unwrap_or_else(|e| panic!("synthetic chain {} must build: {e}", c.id.0));
    }
}
