//! #1007: what a fader drag leaves in `config.yaml`. Every write targets a
//! `tempfile` directory — never the user's real config (#701 / #731).

use std::path::Path;
use std::time::Duration;

use infra_filesystem::{FilesystemStorage, MixerStripConfig};

use super::persist_mixer_strip;
use crate::mixer_persist_coalesce::pending_strips;
use crate::persist_worker::flush;

const MAIN: &str = "out:0,1@hd8";

fn strip(gain_db: f32) -> MixerStripConfig {
    MixerStripConfig {
        id: MAIN.to_string(),
        gain_db,
        muted: false,
        soloed: false,
    }
}

fn mixer_in(config: &Path) -> Vec<MixerStripConfig> {
    FilesystemStorage::load_app_config_at(config)
        .expect("reload")
        .mixer
}

#[test]
fn a_burst_of_fader_values_writes_only_the_last_one() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    for gain_db in [-1.0, -2.0, -3.0] {
        persist_mixer_strip(config.clone(), strip(gain_db));
    }
    flush();

    assert_eq!(mixer_in(&config), vec![strip(-3.0)]);
}

#[test]
fn a_drag_longer_than_the_settle_time_writes_the_drop_value() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    // Each move lands inside the settle window of the previous one, so the
    // queued write keeps waiting instead of writing a mid-drag value.
    for gain_db in [-1.0, -2.0, -3.0, -4.0] {
        persist_mixer_strip(config.clone(), strip(gain_db));
        std::thread::sleep(Duration::from_millis(100));
    }
    flush();

    assert_eq!(mixer_in(&config), vec![strip(-4.0)]);
}

#[test]
fn a_pending_value_withdrawn_before_it_settles_writes_nothing() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    persist_mixer_strip(config.clone(), strip(-6.0));
    assert_eq!(pending_strips().take(&config, MAIN), Some(strip(-6.0)));
    flush();

    assert!(!config.exists(), "nothing was left to write");
}

#[test]
fn a_strip_left_at_unity_never_gets_an_entry() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let config = tmp.path().join("config.yaml");

    persist_mixer_strip(config.clone(), strip(0.0));
    flush();

    assert!(mixer_in(&config).is_empty());
}

#[test]
fn a_failed_write_does_not_stop_later_writes() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    // The parent of this path is a regular file: the write cannot land.
    let blocker = tmp.path().join("not-a-dir");
    std::fs::write(&blocker, b"").expect("blocker file");
    let unwritable = blocker.join("config.yaml");
    let config = tmp.path().join("config.yaml");

    persist_mixer_strip(unwritable.clone(), strip(-5.0));
    flush();
    persist_mixer_strip(config.clone(), strip(-7.0));
    flush();

    assert!(!unwritable.exists());
    assert_eq!(mixer_in(&config), vec![strip(-7.0)]);
}
