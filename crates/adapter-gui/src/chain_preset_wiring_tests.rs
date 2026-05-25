//! Unit tests for `chain_preset_wiring` helpers. Issue #518:
//! - `strip_io_blocks`: the adapter must hand the dispatcher a
//!   preset-blocks list with NO I/O blocks (the dispatcher owns the
//!   chain's I/O across the swap; pre-wrapping here causes duplication).
//! - `default_preset_filename_slug`: the save filename must be the
//!   ACTIVE PRESET's name, not the chain's name (chain.description
//!   moved to `input.label` after #436).
//!
//! Both helpers are pure and live in `chain_preset_wiring.rs`.

use super::{default_preset_filename_slug, strip_io_blocks};

use std::collections::BTreeMap;

use domain::ids::{BlockId, ChainId, DeviceId};
use project::block::{
    AudioBlock, AudioBlockKind, CoreBlock, InputBlock, InputEntry, OutputBlock, OutputEntry,
};
use project::chain::{ChainInputMode, ChainOutputMode};
use project::param::ParameterSet;
use project::rig::{RigInput, RigPreset, RigProject};

fn input_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Input(InputBlock {
            model: "standard".to_string(),
            entries: vec![InputEntry {
                device_id: DeviceId(String::new()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
        }),
    }
}

fn output_block(id: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Output(OutputBlock {
            model: "standard".to_string(),
            entries: vec![OutputEntry {
                device_id: DeviceId(String::new()),
                mode: ChainOutputMode::Stereo,
                channels: vec![0, 1],
            }],
        }),
    }
}

fn core_block(id: &str, model: &str) -> AudioBlock {
    AudioBlock {
        id: BlockId(id.to_string()),
        enabled: true,
        kind: AudioBlockKind::Core(CoreBlock {
            effect_type: "gain".to_string(),
            model: model.to_string(),
            params: ParameterSet::default(),
        }),
    }
}

// ── strip_io_blocks ──────────────────────────────────────────────

#[test]
fn strip_io_blocks_drops_input_and_output_keeps_order_of_core() {
    let kept_a = core_block("a", "ts9");
    let kept_b = core_block("b", "mesa");
    let input = input_block("in");
    let output = output_block("out");
    let blocks = vec![input, kept_a.clone(), kept_b.clone(), output];

    let stripped = strip_io_blocks(blocks);

    assert_eq!(stripped.len(), 2);
    assert_eq!(stripped[0].id.0, "a");
    assert_eq!(stripped[1].id.0, "b");
}

#[test]
fn strip_io_blocks_returns_empty_when_all_blocks_are_io() {
    let blocks = vec![input_block("in"), output_block("out")];
    assert!(strip_io_blocks(blocks).is_empty());
}

#[test]
fn strip_io_blocks_passes_through_when_no_io_present() {
    let a = core_block("a", "ts9");
    let b = core_block("b", "mesa");
    let stripped = strip_io_blocks(vec![a.clone(), b.clone()]);
    assert_eq!(stripped.len(), 2);
    assert_eq!(stripped[0].id.0, "a");
    assert_eq!(stripped[1].id.0, "b");
}

// ── default_preset_filename_slug ─────────────────────────────────

fn rig_with(input_label: Option<&str>, preset_name: Option<&str>) -> RigProject {
    let preset_key = "silverchair-freak".to_string();
    let mut presets = BTreeMap::new();
    presets.insert(
        preset_key.clone(),
        RigPreset {
            id: preset_key.clone(),
            name: preset_name.map(|s| s.to_string()),
            blocks: Vec::new(),
            scene_params: Vec::new(),
            scenes: BTreeMap::new(),
            volume: 100.0,
        },
    );
    let mut bank = BTreeMap::new();
    bank.insert(1usize, preset_key);
    let mut inputs = BTreeMap::new();
    inputs.insert(
        "input-1".to_string(),
        RigInput {
            label: input_label.map(|s| s.to_string()),
            sources: vec![InputEntry {
                device_id: DeviceId(String::new()),
                mode: ChainInputMode::Mono,
                channels: vec![0],
            }],
            bank,
            active_preset: 1,
            active_scene: 1,
            routing: Vec::new(),
        },
    );
    RigProject {
        name: None,
        inputs,
        outputs: BTreeMap::new(),
        presets,
        midi: None,
        chain_order: Vec::new(),
    }
}

#[test]
fn default_filename_uses_active_preset_name_not_chain_label() {
    // chain label = "Scarlett Left" (which the GUI uses as the chain
    // title), preset name = "Silverchair Freak". The save dialog must
    // propose `silverchair_freak`, not `scarlett_left`.
    let rig = rig_with(Some("Scarlett Left"), Some("Silverchair Freak"));
    let slug = default_preset_filename_slug(&ChainId("rig:input-1".to_string()), &rig);
    assert_eq!(slug.as_deref(), Some("silverchair_freak"));
}

#[test]
fn default_filename_falls_back_to_humanized_preset_key_when_name_missing() {
    // Legacy preset with no `name` field — slug from the bank key.
    let rig = rig_with(Some("Scarlett Left"), None);
    let slug = default_preset_filename_slug(&ChainId("rig:input-1".to_string()), &rig);
    // humanize_preset_label("silverchair-freak") = "Silverchair Freak"
    assert_eq!(slug.as_deref(), Some("silverchair_freak"));
}

#[test]
fn default_filename_returns_none_for_non_rig_chain_id() {
    // Non-projected chain (no `rig:` prefix) — caller must keep its
    // own default; we don't invent one.
    let rig = rig_with(Some("X"), Some("P"));
    assert_eq!(
        default_preset_filename_slug(&ChainId("standalone-chain".to_string()), &rig),
        None,
    );
}

#[test]
fn default_filename_returns_none_when_input_missing_from_rig() {
    let rig = rig_with(Some("X"), Some("P"));
    assert_eq!(
        default_preset_filename_slug(&ChainId("rig:input-999".to_string()), &rig),
        None,
    );
}
