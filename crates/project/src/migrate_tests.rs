//! Tests for legacy `Project` → `RigProject` migration (#450).
//!
//! Model A (#716): device endpoints are NOT carried by migration — the
//! per-machine binding registry owns them and the user re-selects bindings
//! after load. These tests assert the structural migration only (one input
//! per chain, preset banks, processing-block preservation, volume,
//! placeholder routing), never device data.

use super::*;
use crate::block::{AudioBlock, AudioBlockKind, CoreBlock, InputBlock, OutputBlock};
use crate::chain::Chain;
use crate::param::ParameterSet;
use domain::ids::{BlockId, ChainId};

/// A legacy Input block. Under model A it carries no device endpoints, so the
/// migration only needs its presence to know the chain had an input.
fn input_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
        }),
    }
}

fn output_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.into()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".into(),
            io: String::new(),
            endpoint: String::new(),
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
        di_output: None,
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
fn migrate_each_chain_becomes_its_own_input() {
    // Model A: with no device data to group on, every legacy chain becomes
    // one input (`input-{N}`) holding a single-preset bank.
    let p = legacy(vec![
        chain("song-a", 110.0, vec![input_block("i1"), fx("a")]),
        chain("song-b", 120.0, vec![input_block("i2"), fx("b")]),
        chain("voice", 100.0, vec![input_block("i3"), fx("c")]),
    ]);
    let r = migrate_legacy_project(&p);

    assert_eq!(r.inputs.len(), 3, "one input per chain");
    let a = r.inputs.get("input-1").expect("input-1");
    assert_eq!(a.bank.len(), 1);
    assert_eq!(a.bank.get(&1).map(String::as_str), Some("song-a"));
    assert_eq!(a.active_preset, 1);
    assert_eq!(a.active_scene, 1);
    assert_eq!(r.presets.len(), 3, "each chain becomes a preset");
    assert_eq!(
        r.presets.get("song-b").unwrap().volume,
        120.0,
        "per-preset volume preserved (invariant #10)"
    );
}

#[test]
fn migrate_each_chain_becomes_input_preset_bank() {
    let p = legacy(vec![
        chain(
            "clean",
            100.0,
            vec![input_block("i1"), fx("a"), output_block("o1")],
        ),
        chain(
            "drive",
            100.0,
            vec![input_block("i2"), fx("b"), output_block("o2")],
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
fn migrate_does_not_carry_device_bindings() {
    // Old projects load WITHOUT device data: io/endpoint/binding-ids are all
    // empty so the user re-selects bindings in the I/O editor.
    let p = legacy(vec![chain(
        "clean",
        100.0,
        vec![input_block("i"), fx("a"), output_block("o")],
    )]);
    let r = migrate_legacy_project(&p);
    let input = r.inputs.get("input-1").expect("input-1");
    assert!(input.io.is_empty());
    assert!(input.endpoint.is_empty());
    assert!(input.io_binding_ids.is_empty());
    for output in r.outputs.values() {
        assert!(output.io.is_empty());
        assert!(output.endpoint.is_empty());
    }
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
            input_block("i"),
            fx("first"),
            fx("second"),
            output_block("o"),
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
fn migrate_creates_placeholder_output_per_output_block_and_routes_to_it() {
    // Device endpoints are gone, so each output block on a chain becomes one
    // structural placeholder output the input routes to. Cross-references
    // still resolve in `validate`.
    let p = legacy(vec![
        chain("a", 100.0, vec![fx("x"), output_block("o1")]),
        chain("b", 100.0, vec![fx("y"), output_block("o2")]),
    ]);
    let r = migrate_legacy_project(&p);
    assert_eq!(r.outputs.len(), 2, "one placeholder output per output block");
    assert_eq!(
        r.inputs.get("input-1").unwrap().routing,
        vec!["output-1".to_string()]
    );
    assert_eq!(
        r.inputs.get("input-2").unwrap().routing,
        vec!["output-2".to_string()]
    );
    r.validate().expect("routing cross-refs must resolve");
}

#[test]
fn migrate_result_is_valid() {
    let p = legacy(vec![chain(
        "clean",
        100.0,
        vec![input_block("i"), fx("a"), output_block("o")],
    )]);
    migrate_legacy_project(&p)
        .validate()
        .expect("migrated project must be valid");
}

#[test]
fn migrate_is_deterministic_idempotent() {
    let p = legacy(vec![
        chain("clean", 100.0, vec![input_block("i"), fx("a")]),
        chain("drive", 120.0, vec![fx("b"), output_block("o")]),
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

// #436: the select showed the slug ("studio-clean-compressor") instead
// of a description. A RigPreset must carry a human `name` (the original
// chain description) AND an `id` (the stable pool key). Migration fills
// both; the UI shows `name`.
#[test]
fn migrate_carries_chain_description_as_preset_name_and_id() {
    let p = legacy(vec![chain(
        "Studio Clean Compressor",
        100.0,
        vec![input_block("i"), fx("a")],
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
