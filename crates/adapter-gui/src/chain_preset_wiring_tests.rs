//! Unit tests for `chain_preset_wiring` helpers. Issue #518:
//! - `strip_io_blocks`: the adapter must hand the dispatcher a
//!   preset-blocks list with NO I/O blocks (the dispatcher owns the
//!   chain's I/O across the swap; pre-wrapping here causes duplication).
//! - `default_preset_filename_slug`: the save filename must be the
//!   ACTIVE PRESET's name, not the chain's name (chain.description
//!   moved to `input.label` after #436).
//!
//! Both helpers are pure and live in `chain_preset_wiring.rs`.

use super::{
    default_preset_filename_slug, filter_preset_names, preset_filename, preset_overwrite_required,
    preset_rename_target_from_path, preset_save_path, strip_io_blocks,
};

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
    // propose the preset name VERBATIM (issue #510 user feedback: no
    // slug transformation; the file mirrors what the user sees).
    let rig = rig_with(Some("Scarlett Left"), Some("Silverchair Freak"));
    let name = default_preset_filename_slug(&ChainId("rig:input-1".to_string()), &rig);
    assert_eq!(name.as_deref(), Some("Silverchair Freak"));
}

#[test]
fn default_filename_falls_back_to_humanized_preset_key_when_name_missing() {
    // Legacy preset with no `name` field — fall back to humanized key.
    let rig = rig_with(Some("Scarlett Left"), None);
    let name = default_preset_filename_slug(&ChainId("rig:input-1".to_string()), &rig);
    // humanize_preset_label("silverchair-freak") = "Silverchair Freak"
    assert_eq!(name.as_deref(), Some("Silverchair Freak"));
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

// ── preset_rename_target_from_path (issue #510) ─────────────────

#[test]
fn rename_target_returns_raw_file_stem_preserving_dashes() {
    // Issue #510 (user feedback): the active preset's name must
    // match the file's stem VERBATIM. Earlier humanization replaced
    // '-' with ' ' which silently rewrote the user's filename.
    use std::path::PathBuf;
    let path = PathBuf::from("/presets/lead-boost.yaml");
    assert_eq!(
        preset_rename_target_from_path(&path).as_deref(),
        Some("lead-boost"),
    );
}

#[test]
fn rename_target_preserves_underscores_and_case() {
    use std::path::PathBuf;
    let path = PathBuf::from("/presets/silverchair_freak.yaml");
    assert_eq!(
        preset_rename_target_from_path(&path).as_deref(),
        Some("silverchair_freak"),
    );
}

#[test]
fn rename_target_preserves_spaces_in_filename() {
    use std::path::PathBuf;
    let path = PathBuf::from("/presets/Lead Boost.yaml");
    assert_eq!(
        preset_rename_target_from_path(&path).as_deref(),
        Some("Lead Boost"),
    );
}

#[test]
fn rename_target_returns_none_for_empty_path() {
    use std::path::PathBuf;
    assert_eq!(preset_rename_target_from_path(&PathBuf::from("")), None);
}

// ── preset_filename / preset_save_path (issue #510) ─────────────

#[test]
fn preset_filename_preserves_name_verbatim() {
    // Issue #510 (user feedback): the on-disk filename must mirror
    // the user-visible name 1:1. Earlier we lowercased + replaced
    // spaces — the file then renamed itself to a slug on save and
    // on reload the combobox showed the slug, surprising the user.
    assert_eq!(
        preset_filename("Silverchair Freak"),
        "Silverchair Freak.yaml",
    );
}

#[test]
fn preset_filename_trims_outer_whitespace_only() {
    assert_eq!(preset_filename("  Lead Boost  "), "Lead Boost.yaml");
}

#[test]
fn preset_filename_replaces_filesystem_illegal_chars_with_underscore() {
    // `/ \ : * ? " < > |` are forbidden on at least one supported OS.
    // Other characters (spaces, dashes, dots, accents) survive.
    assert_eq!(preset_filename("rig/cool?"), "rig_cool_.yaml");
}

#[test]
fn preset_filename_preserves_dash_with_spaces_around_it() {
    // User feedback: "FOO FIGHTERS HEROS" -> "FOO FIGHTERS - HEROS"
    // must persist to disk with the dash and the spaces intact.
    assert_eq!(
        preset_filename("FOO FIGHTERS - HEROS"),
        "FOO FIGHTERS - HEROS.yaml",
    );
}

#[test]
fn save_path_joins_configured_dir_with_filename() {
    use std::path::PathBuf;
    let dir = PathBuf::from("/data/openrig/presets");
    assert_eq!(
        preset_save_path(&dir, "Lead Boost"),
        PathBuf::from("/data/openrig/presets/Lead Boost.yaml"),
    );
}

// ── filter_preset_names (issue #510 load search) ────────────────

#[test]
fn filter_empty_query_returns_all_names() {
    let names = vec!["A".to_string(), "B".to_string()];
    let r = filter_preset_names(&names, "");
    assert_eq!(r.len(), 2);
}

#[test]
fn filter_is_case_insensitive_substring_match() {
    let names = vec![
        "Silverchair Freak".to_string(),
        "Clean".to_string(),
        "Lead Boost".to_string(),
    ];
    let r = filter_preset_names(&names, "FREAK");
    assert_eq!(r, vec![&"Silverchair Freak".to_string()]);
}

#[test]
fn filter_no_match_returns_empty() {
    let names = vec!["a".to_string(), "b".to_string()];
    assert!(filter_preset_names(&names, "xyz").is_empty());
}

// ── preset_overwrite_required (issue #510 save overwrite modal) ─

#[test]
fn overwrite_false_when_target_does_not_exist() {
    use std::path::PathBuf;
    let dir = PathBuf::from("/this/path/should/not/exist/in/any/sane/repo");
    assert!(!preset_overwrite_required(&dir, "definitely_missing"));
}
