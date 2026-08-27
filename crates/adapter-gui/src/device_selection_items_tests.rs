//! The device rows the Settings panel binds to.
//!
//! Pure: descriptors in, rows out. A device the user already configured keeps
//! its (normalized) settings; one seen for the first time gets the defaults;
//! and the selected flag follows what `gui-settings.yaml` says was chosen.

use super::{build_device_selection_items, mark_unselected_devices};
use crate::device_settings_resolve::default_device_settings;
use crate::{DEFAULT_BUFFER_SIZE_FRAMES, DEFAULT_SAMPLE_RATE};
use domain::AudioDeviceDescriptor;
use slint::{Model, VecModel};
use std::rc::Rc;

fn descriptor(id: &str, name: &str) -> AudioDeviceDescriptor {
    AudioDeviceDescriptor {
        id: id.to_string(),
        name: name.to_string(),
        channels: 2,
    }
}

#[test]
fn a_device_with_no_saved_config_gets_the_defaults() {
    let items = build_device_selection_items(&[descriptor("dev-a", "Scarlett")], &[]);

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name.as_str(), "Scarlett");
    assert_eq!(
        items[0].sample_rate_text.as_str(),
        DEFAULT_SAMPLE_RATE.to_string()
    );
    assert_eq!(
        items[0].buffer_size_text.as_str(),
        DEFAULT_BUFFER_SIZE_FRAMES.to_string()
    );
}

#[test]
fn a_saved_config_is_carried_into_the_row_after_normalization() {
    let mut saved = default_device_settings("dev-a".into(), "Scarlett".into());
    saved.sample_rate = 44_100;
    saved.buffer_size_frames = 9_999; // never supported → falls back

    let items = build_device_selection_items(&[descriptor("dev-a", "Scarlett")], &[saved]);

    assert_eq!(items[0].sample_rate_text.as_str(), "44100");
    assert_eq!(
        items[0].buffer_size_text.as_str(),
        DEFAULT_BUFFER_SIZE_FRAMES.to_string(),
        "an unsupported buffer never reaches the picker"
    );
}

#[test]
fn only_the_devices_the_user_chose_stay_selected() {
    let items = build_device_selection_items(
        &[
            descriptor("dev-a", "Scarlett"),
            descriptor("dev-b", "Built-in"),
        ],
        &[],
    );
    let model = Rc::new(VecModel::from(items));
    let chosen = default_device_settings("dev-b".into(), "Built-in".into());

    mark_unselected_devices(&model, &[chosen]);

    assert!(
        !model.row_data(0).unwrap().selected,
        "Scarlett was not chosen"
    );
    assert!(model.row_data(1).unwrap().selected, "Built-in was");
}

#[test]
fn an_empty_descriptor_list_yields_no_rows() {
    assert!(build_device_selection_items(&[], &[]).is_empty());
}
