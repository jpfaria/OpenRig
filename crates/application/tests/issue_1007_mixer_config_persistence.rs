//! #1007: the global mixer's faders and mutes live in the per-machine SYSTEM
//! `config.yaml` (ADR 0003 — a monitor level belongs to this desk, not to a
//! `project.yaml` that travels to another machine).
//!
//! Every write targets a `tempfile` directory — never the user's real config
//! (#701 / #731).

use std::path::Path;

use application::app_config_persist::persist_mixer_strip;
use infra_filesystem::{FilesystemStorage, MixerStripConfig};

const MAIN: &str = "out:0,1@hd8";
const GUITAR: &str = "in:0@hd8";

fn strip(id: &str, gain_db: f32, muted: bool) -> MixerStripConfig {
    MixerStripConfig {
        id: id.to_string(),
        gain_db,
        muted,
        soloed: false,
    }
}

fn write(path: &Path, value: MixerStripConfig) {
    persist_mixer_strip(path.to_path_buf(), value);
    application::persist_worker::flush();
}

#[test]
fn a_moved_fader_survives_a_reload() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write(&config, strip(MAIN, -7.5, false));
    write(&config, strip(GUITAR, 0.0, true));

    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert_eq!(
        reloaded.mixer,
        vec![strip(MAIN, -7.5, false), strip(GUITAR, 0.0, true)]
    );
}

#[test]
fn a_second_write_to_the_same_strip_replaces_it() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write(&config, strip(MAIN, -7.5, false));
    write(&config, strip(MAIN, -3.0, true));

    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert_eq!(reloaded.mixer, vec![strip(MAIN, -3.0, true)]);
}

#[test]
fn a_strip_back_at_unity_leaves_the_config() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write(&config, strip(MAIN, -7.5, false));
    write(&config, strip(MAIN, 0.0, false));

    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert!(reloaded.mixer.is_empty(), "got {:?}", reloaded.mixer);
}

#[test]
fn a_whole_config_resave_keeps_the_mixer() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    write(&config, strip(MAIN, -7.5, true));
    FilesystemStorage::update_app_config_at(&config, |c| c.language = Some("pt-BR".into()))
        .expect("unrelated save");

    let reloaded = FilesystemStorage::load_app_config_at(&config).expect("reload");
    assert_eq!(reloaded.mixer, vec![strip(MAIN, -7.5, true)]);
}
