//! #14: the metronome's settings live in the per-machine SYSTEM `config.yaml`
//! (ADR 0003 — a practice tempo does not travel inside a `.openrig`).
//!
//! Two contracts are pinned here.
//!
//! 1. What IS persisted survives a reload AND an unrelated whole-config
//!    re-save. Dropping a section on a whole-config write is the trap that
//!    already bit #607 (path overrides) and #627 (buffer size): every other
//!    setting goes through a load → mutate → save cycle, so a metronome field
//!    that does not round-trip disappears the first time the user changes
//!    anything else.
//! 2. `enabled` is NEVER written. The app always opens with the metronome
//!    off, so no config file may be able to make it click on boot.
//!
//! Every write targets a `tempfile` directory — never the user's real config
//! (#701 / #731).

use std::path::Path;

use application::app_config_persist::persist_metronome;
use infra_filesystem::{FilesystemStorage, MetronomeConfig};

/// A non-default value for every persisted field, so a field that silently
/// falls back to its default is visible in the assertion.
fn tuned() -> MetronomeConfig {
    MetronomeConfig {
        bpm: 92.0,
        beats_per_bar: 7,
        subdivision: "triplets".to_string(),
        timbre: "wood".to_string(),
        volume: 0.35,
        count_in: true,
        output_device: Some("hw:1,0".to_string()),
    }
}

/// Persist `settings` at `path` and wait for the worker to land the write.
fn write_settings(path: &Path, settings: MetronomeConfig) {
    persist_metronome(Some(path.to_path_buf()), move |m| *m = settings);
    application::persist_worker::flush();
}

#[test]
fn settings_survive_a_reload() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write_settings(&config, tuned());

    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload config");
    assert_eq!(
        reloaded.metronome,
        tuned(),
        "every persisted metronome field must come back from config.yaml"
    );
}

#[test]
fn enabled_is_never_written() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write_settings(&config, tuned());

    let raw = std::fs::read_to_string(&config).expect("config.yaml must exist");
    let parsed: serde_yaml::Value = serde_yaml::from_str(&raw).expect("config.yaml must parse");
    let section = parsed
        .get("metronome")
        .expect("config.yaml must carry a metronome section");
    assert!(
        section.get("enabled").is_none(),
        "the metronome on/off flag must never be persisted — the app always \
         opens with it off. Got:\n{raw}"
    );
}

#[test]
fn a_whole_config_resave_preserves_metronome_settings() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write_settings(&config, tuned());

    // The #607/#627 shape: an unrelated setting goes through its own
    // load → mutate → save cycle over the WHOLE config.
    FilesystemStorage::update_app_config_at(&config, |c| c.language = Some("pt-BR".to_string()))
        .expect("unrelated whole-config save");

    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload config");
    assert_eq!(
        reloaded.metronome,
        tuned(),
        "changing an unrelated setting must not drop the metronome section"
    );
    assert_eq!(reloaded.language.as_deref(), Some("pt-BR"));
}

#[test]
fn a_missing_metronome_section_loads_defaults() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");
    // A config.yaml written before the metronome existed.
    std::fs::write(&config, "language: en-US\n").expect("seed legacy config");

    let loaded = FilesystemStorage::load_app_config_at(&config).expect("load legacy config");
    assert_eq!(
        loaded.metronome,
        MetronomeConfig::default(),
        "a config.yaml with no metronome section must load the defaults, \
         not fail and not zero the settings"
    );
}
