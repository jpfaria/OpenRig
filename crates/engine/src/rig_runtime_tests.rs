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
            .map(|(k, b)| (k.to_string(), RigPreset { blocks: b }))
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
