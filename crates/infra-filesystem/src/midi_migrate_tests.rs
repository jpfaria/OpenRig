//! Tests for legacy `midi-map.yaml` → `midi-profile.yaml` + `midi-bindings.yaml`
//! migration (ADR 0003 / #499, C5).
//!
//! The migration runs once on first load: it splits the legacy single file
//! into the system device profile (`input:`) and a system bindings fallback
//! (`bindings:`), then deletes the legacy file so the split is durable.

use crate::midi_migrate::{migrate_legacy_midi_map, MigrationOutcome};
use std::fs;
use std::path::PathBuf;

fn tmp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("openrig_tests_midi_migrate")
        .join(test_name)
        .join(format!("{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("failed to create tmp dir");
    dir
}

#[test]
fn missing_legacy_file_is_noop() {
    let dir = tmp_dir("missing_legacy_file_is_noop");
    let legacy = dir.join("midi-map.yaml");
    let profile = dir.join("midi-profile.yaml");
    let bindings = dir.join("midi-bindings.yaml");

    let outcome = migrate_legacy_midi_map(&legacy, &profile, &bindings).unwrap();
    assert!(matches!(outcome, MigrationOutcome::NoLegacyFile));
    assert!(!profile.exists());
    assert!(!bindings.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn full_legacy_file_splits_into_profile_plus_bindings() {
    let dir = tmp_dir("full_legacy_file_splits");
    let legacy = dir.join("midi-map.yaml");
    let profile = dir.join("midi-profile.yaml");
    let bindings = dir.join("midi-bindings.yaml");

    fs::write(
        &legacy,
        "\
input: Chocolate
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: SaveProject
  - source: { kind: cc, channel: 1, controller: 7 }
    command: SetChainVolume
    args: { chain: \"rig:guitar\" }
    scale: { min: 0.0, max: 200.0 }
",
    )
    .unwrap();

    let outcome = migrate_legacy_midi_map(&legacy, &profile, &bindings).unwrap();
    assert!(matches!(outcome, MigrationOutcome::Migrated));

    // Legacy file removed after a successful split.
    assert!(
        !legacy.exists(),
        "legacy file should be deleted after split"
    );

    // Profile contains the input substring, no bindings.
    let prof_raw = fs::read_to_string(&profile).unwrap();
    assert!(prof_raw.contains("input: Chocolate"), "{prof_raw}");
    assert!(
        !prof_raw.contains("bindings"),
        "profile must not carry bindings: {prof_raw}"
    );

    // Bindings file contains bindings, no input.
    let bind_raw = fs::read_to_string(&bindings).unwrap();
    assert!(bind_raw.contains("bindings"), "{bind_raw}");
    assert!(
        !bind_raw.contains("input:"),
        "bindings file must not carry input: {bind_raw}"
    );
    assert!(bind_raw.contains("SaveProject"));
    assert!(bind_raw.contains("SetChainVolume"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn legacy_file_with_only_input_writes_profile_and_no_bindings_file() {
    let dir = tmp_dir("legacy_only_input");
    let legacy = dir.join("midi-map.yaml");
    let profile = dir.join("midi-profile.yaml");
    let bindings = dir.join("midi-bindings.yaml");

    fs::write(&legacy, "input: Chocolate\n").unwrap();

    let outcome = migrate_legacy_midi_map(&legacy, &profile, &bindings).unwrap();
    assert!(matches!(outcome, MigrationOutcome::Migrated));
    assert!(profile.exists());
    assert!(
        !bindings.exists(),
        "no bindings to persist → no fallback file"
    );
    assert!(!legacy.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn legacy_file_with_only_bindings_writes_bindings_and_no_profile_file() {
    let dir = tmp_dir("legacy_only_bindings");
    let legacy = dir.join("midi-map.yaml");
    let profile = dir.join("midi-profile.yaml");
    let bindings = dir.join("midi-bindings.yaml");

    fs::write(
        &legacy,
        "\
bindings:
  - source: { kind: note_on, channel: 1, note: 60 }
    command: SaveProject
",
    )
    .unwrap();

    let outcome = migrate_legacy_midi_map(&legacy, &profile, &bindings).unwrap();
    assert!(matches!(outcome, MigrationOutcome::Migrated));
    assert!(bindings.exists());
    assert!(
        !profile.exists(),
        "no input to persist → no profile file (defaults will apply)"
    );
    assert!(!legacy.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn malformed_legacy_file_is_skipped_and_left_in_place() {
    let dir = tmp_dir("malformed_legacy");
    let legacy = dir.join("midi-map.yaml");
    let profile = dir.join("midi-profile.yaml");
    let bindings = dir.join("midi-bindings.yaml");

    fs::write(&legacy, "this is not: { valid yaml at all\n").unwrap();

    let outcome = migrate_legacy_midi_map(&legacy, &profile, &bindings).unwrap();
    assert!(matches!(outcome, MigrationOutcome::SkippedMalformed));
    assert!(legacy.exists(), "malformed file must be left in place");
    assert!(!profile.exists());
    assert!(!bindings.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn empty_legacy_file_is_migrated_to_empty_state_and_deleted() {
    let dir = tmp_dir("empty_legacy");
    let legacy = dir.join("midi-map.yaml");
    let profile = dir.join("midi-profile.yaml");
    let bindings = dir.join("midi-bindings.yaml");

    fs::write(&legacy, "{}\n").unwrap();

    let outcome = migrate_legacy_midi_map(&legacy, &profile, &bindings).unwrap();
    assert!(matches!(outcome, MigrationOutcome::Migrated));
    assert!(!legacy.exists());
    assert!(!profile.exists());
    assert!(!bindings.exists());
    let _ = fs::remove_dir_all(&dir);
}
