//! #1007: a fader drag writes `config.yaml` once per queued write, never once
//! per pixel, and the write carries the drop value.

use std::path::PathBuf;

use infra_filesystem::MixerStripConfig;

use super::PendingStrips;

fn strip(id: &str, gain_db: f32) -> MixerStripConfig {
    MixerStripConfig {
        id: id.into(),
        gain_db,
        muted: false,
        soloed: false,
    }
}

#[test]
fn a_drag_queues_one_write_carrying_the_last_value() {
    let mut pending = PendingStrips::default();
    let path = PathBuf::from("config.yaml");
    let queued = (0..60)
        .filter(|i| pending.submit(path.clone(), strip("in:0@d", -(*i as f32) / 2.0)))
        .count();
    assert_eq!(queued, 1, "60 fader ticks queued {queued} config writes");
    assert_eq!(pending.take(&path, "in:0@d"), Some(strip("in:0@d", -29.5)));
    assert_eq!(pending.take(&path, "in:0@d"), None);
}

#[test]
fn a_new_move_after_the_write_ran_queues_again() {
    let mut pending = PendingStrips::default();
    let path = PathBuf::from("config.yaml");
    assert!(pending.submit(path.clone(), strip("in:0@d", -3.0)));
    pending.take(&path, "in:0@d");
    assert!(pending.submit(path.clone(), strip("in:0@d", -4.0)));
}

#[test]
fn two_strips_each_get_their_own_write() {
    let mut pending = PendingStrips::default();
    let path = PathBuf::from("config.yaml");
    assert!(pending.submit(path.clone(), strip("in:0@d", -3.0)));
    assert!(pending.submit(path.clone(), strip("out:0,1@d", -6.0)));
    assert!(!pending.submit(path.clone(), strip("in:0@d", -5.0)));
    assert_eq!(
        pending.take(&path, "out:0,1@d"),
        Some(strip("out:0,1@d", -6.0))
    );
    assert_eq!(pending.take(&path, "in:0@d"), Some(strip("in:0@d", -5.0)));
}
