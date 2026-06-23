//! Tests for legacy `Project` → `RigProject` migration (#450).

use super::*;
use crate::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use crate::chain::{Chain, ChainInputMode, ChainOutputMode};
use crate::param::ParameterSet;
use domain::ids::{BlockId, ChainId, DeviceId};

fn input_block(id: &str, entries: Vec<(&str, Vec<usize>)>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: entries
                .into_iter()
                .map(|(d, ch)| InputEntry {
                    device_id: DeviceId(d.into()),
                    mode: ChainInputMode::Mono,
                    channels: ch,
                })
                .collect(),
        }),
    }
}

fn output_block(id: &str, device: &str, channels: Vec<usize>) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
            entries: vec![OutputEntry {
                device_id: DeviceId(device.into()),
                mode: ChainOutputMode::Stereo,
                channels,
            }],
        }),
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

fn chain(desc: &str, volume: f32, blocks: Vec<AudioBlock>) -> Chain {
    Chain {
        id: ChainId(format!("chain:{desc}")),
        description: Some(desc.into()),
        instrument: "electric_guitar".into(),
        enabled: true,
        volume,
        io_binding_ids: vec![],
        blocks,
    }
}

fn legacy(chains: Vec<Chain>) -> Project {
    Project {
        name: Some("Studio".into()),
        device_settings: vec![],
        chains,
        midi: None,
    }
}

#[test]
fn migrate_groups_same_source_chains_into_one_input_bank() {
    // The real-world case: many songs on the same guitar input → ONE
    // input with a bank of N presets (not N inputs).
    let p = legacy(vec![
        chain(
            "song-a",
            110.0,
            vec![input_block("i1", vec![("sc", vec![0])]), fx("a")],
        ),
        chain(
            "song-b",
            120.0,
            vec![input_block("i2", vec![("sc", vec![0])]), fx("b")], // same source
        ),
        chain(
            "voice",
            100.0,
            vec![input_block("i3", vec![("sc", vec![1])]), fx("c")], // different (ch1)
        ),
    ]);
    let r = migrate_legacy_project(&p);

    assert_eq!(r.inputs.len(), 2, "two distinct sources → two inputs");
    let g = r.inputs.get("input-1").expect("input-1 (ch0 group)");
    assert_eq!(g.bank.len(), 2, "both ch0 chains share one bank");
    assert_eq!(g.bank.get(&1).map(String::as_str), Some("song-a"));
    assert_eq!(g.bank.get(&2).map(String::as_str), Some("song-b"));
    assert_eq!(g.active_preset, 1);
    assert_eq!(g.active_scene, 1);
    let v = r.inputs.get("input-2").expect("input-2 (ch1)");
    assert_eq!(v.bank.len(), 1);
    assert_eq!(r.presets.len(), 3, "each chain still becomes a preset");
    assert_eq!(
        r.presets.get("song-b").unwrap().volume,
        120.0,
        "per-preset volume preserved (invariant #10)"
    );
}

#[test]
fn migrate_mono_multichannel_normalizes_to_first_channel() {
    // `mode: mono, channels: [0,1]` (a data quirk) normalizes to [0] and
    // groups with the plain ch0 input.
    let p = legacy(vec![
        chain(
            "a",
            100.0,
            vec![input_block("i1", vec![("sc", vec![0])]), fx("a")],
        ),
        chain(
            "b",
            100.0,
            vec![input_block("i2", vec![("sc", vec![0, 1])]), fx("b")],
        ),
    ]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.inputs.len(), 1, "mono [0,1] ≡ mono [0] → same input");
    assert_eq!(r.inputs.get("input-1").unwrap().bank.len(), 2);
    assert_eq!(
        r.inputs.get("input-1").unwrap().sources[0].channels,
        vec![0],
        "stored source normalized to the single mono channel"
    );
}

#[test]
fn migrate_each_chain_becomes_input_preset_bank() {
    let p = legacy(vec![
        chain(
            "clean",
            100.0,
            vec![
                input_block("i1", vec![("sc", vec![0])]),
                fx("a"),
                output_block("o1", "sc", vec![0, 1]),
            ],
        ),
        chain(
            "drive",
            100.0,
            vec![
                input_block("i2", vec![("sc", vec![1])]),
                fx("b"),
                output_block("o2", "sc", vec![0, 1]),
            ],
        ),
    ]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.inputs.len(), 2);
    assert_eq!(r.presets.len(), 2);
    let i1 = r.inputs.get("input-1").expect("input-1");
    assert_eq!(i1.active_preset, 1);
    assert_eq!(i1.active_scene, 1);
    assert_eq!(i1.bank.get(&1).map(String::as_str), Some("clean"));
}

#[test]
fn migrate_loses_no_preset() {
    let p = legacy(vec![
        chain("a", 100.0, vec![fx("x")]),
        chain("b", 100.0, vec![fx("y")]),
        chain("c", 100.0, vec![fx("z")]),
    ]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.presets.len(), 3, "every chain must become a preset");
    let banked: Vec<&String> = r.inputs.values().flat_map(|i| i.bank.values()).collect();
    for name in r.presets.keys() {
        assert!(banked.contains(&name), "preset {name} not in any bank");
    }
}

#[test]
fn migrate_strips_io_and_preserves_processing_order() {
    let p = legacy(vec![chain(
        "clean",
        100.0,
        vec![
            input_block("i", vec![("sc", vec![0])]),
            fx("first"),
            fx("second"),
            output_block("o", "sc", vec![0, 1]),
        ],
    )]);
    let r = migrate_legacy_project(&p);
    let preset = r.presets.get("clean").unwrap();
    assert_eq!(preset.blocks.len(), 2, "I/O stripped, fx kept");
    let ids: Vec<&str> = preset.blocks.iter().map(|b| b.id.0.as_str()).collect();
    assert_eq!(ids, vec!["first", "second"], "order preserved");
    for b in &preset.blocks {
        assert!(!matches!(
            b.kind,
            AudioBlockKind::Input(_) | AudioBlockKind::Output(_)
        ));
    }
}

#[test]
fn migrate_carries_volume() {
    let p = legacy(vec![chain("loud", 150.0, vec![fx("x")])]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.presets.get("loud").unwrap().volume, 150.0);
}

#[test]
fn migrate_dedups_outputs_and_sets_routing() {
    let p = legacy(vec![
        chain(
            "a",
            100.0,
            vec![fx("x"), output_block("o1", "pa", vec![0, 1])],
        ),
        chain(
            "b",
            100.0,
            vec![fx("y"), output_block("o2", "pa", vec![0, 1])],
        ),
    ]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.outputs.len(), 1, "identical outputs deduped");
    let out_name = r.outputs.keys().next().unwrap().clone();
    for input in r.inputs.values() {
        assert_eq!(input.routing, vec![out_name.clone()]);
    }
}

#[test]
fn migrate_result_is_valid() {
    let p = legacy(vec![chain(
        "clean",
        100.0,
        vec![
            input_block("i", vec![("sc", vec![0])]),
            fx("a"),
            output_block("o", "sc", vec![0, 1]),
        ],
    )]);
    migrate_legacy_project(&p)
        .validate()
        .expect("migrated project must be valid");
}

#[test]
fn migrate_is_deterministic_idempotent() {
    let p = legacy(vec![
        chain(
            "clean",
            100.0,
            vec![input_block("i", vec![("sc", vec![0])]), fx("a")],
        ),
        chain(
            "drive",
            120.0,
            vec![fx("b"), output_block("o", "sc", vec![2, 3])],
        ),
    ]);
    assert_eq!(migrate_legacy_project(&p), migrate_legacy_project(&p));
}

#[test]
fn migrate_empty_project_is_empty_rig() {
    let r = migrate_legacy_project(&legacy(vec![]));
    assert!(r.inputs.is_empty());
    assert!(r.outputs.is_empty());
    assert!(r.presets.is_empty());
    assert_eq!(r.name.as_deref(), Some("Studio"));
}

#[test]
fn migrate_preserves_multi_source() {
    let p = legacy(vec![chain(
        "dual",
        100.0,
        vec![
            input_block("i", vec![("sc", vec![0]), ("sc", vec![1])]),
            fx("a"),
        ],
    )]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.inputs.get("input-1").unwrap().sources.len(), 2);
}

// #436: the select showed the slug ("studio-clean-compressor") instead
// of a description. A RigPreset must carry a human `name` (the original
// chain description) AND an `id` (the stable pool key). Migration fills
// both; the UI shows `name`.
#[test]
fn migrate_carries_chain_description_as_preset_name_and_id() {
    let p = legacy(vec![chain(
        "Studio Clean Compressor",
        100.0,
        vec![input_block("i", vec![("sc", vec![0])]), fx("a")],
    )]);
    let r = migrate_legacy_project(&p);

    let input = r.inputs.get("input-1").expect("input");
    let key = input.bank.values().next().expect("a banked preset").clone();
    let preset = r.presets.get(&key).expect("preset in pool");

    assert_eq!(
        preset.name.as_deref(),
        Some("Studio Clean Compressor"),
        "name must be the human chain description, not the slug"
    );
    assert_eq!(
        preset.id, key,
        "id must equal the stable pool key the bank references"
    );
}
