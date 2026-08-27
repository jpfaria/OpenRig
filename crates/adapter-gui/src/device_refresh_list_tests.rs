//! #913 — refreshing the list of devices the host reports.
//!
//! The picker's model and the descriptor list the caller keeps must always
//! agree: one row per device, in the same order, with the device's own name.
//! A refresh also has to REPLACE the previous rows — appending would grow the
//! picker every time the user re-scanned after a hot-swap.

use super::{
    ensure_devices_loaded, invalidate_device_cache, refresh_input_devices, refresh_output_devices,
};
use domain::AudioDeviceDescriptor;
use slint::{Model, SharedString, VecModel};
use std::cell::RefCell;
use std::rc::Rc;

fn model() -> Rc<VecModel<SharedString>> {
    Rc::new(VecModel::from(Vec::<SharedString>::new()))
}

fn names(model: &Rc<VecModel<SharedString>>) -> Vec<String> {
    (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn an_input_refresh_publishes_one_row_per_device_in_order() {
    let m = model();
    let devices = refresh_input_devices(&m);
    assert_eq!(
        names(&m),
        devices.iter().map(|d| d.name.clone()).collect::<Vec<_>>(),
        "the picker shows exactly what the caller was handed"
    );
}

#[test]
fn an_output_refresh_publishes_one_row_per_device_in_order() {
    let m = model();
    let devices = refresh_output_devices(&m);
    assert_eq!(
        names(&m),
        devices.iter().map(|d| d.name.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn a_second_refresh_replaces_the_rows_instead_of_appending() {
    let m = model();
    m.set_vec(vec![SharedString::from("stale device")]);
    let devices = refresh_input_devices(&m);
    assert_eq!(m.row_count(), devices.len());
    assert!(!names(&m).contains(&"stale device".to_string()));
}

#[test]
fn a_refresh_after_invalidating_the_cache_still_answers() {
    // Dropping the snapshot must not leave the picker empty — the stale list
    // is served while the background refresh runs (#693).
    let m = model();
    let before = refresh_input_devices(&m).len();
    invalidate_device_cache();
    let after = refresh_input_devices(&m).len();
    assert_eq!(before, after, "the device set did not change under us");
}

#[test]
fn devices_already_loaded_are_not_re_enumerated() {
    let input = Rc::new(RefCell::new(vec![AudioDeviceDescriptor {
        id: "seeded-in".into(),
        name: "Seeded In".into(),
        channels: 2,
    }]));
    let output = Rc::new(RefCell::new(vec![AudioDeviceDescriptor {
        id: "seeded-out".into(),
        name: "Seeded Out".into(),
        channels: 2,
    }]));
    ensure_devices_loaded(&input, &output);
    assert_eq!(
        input.borrow()[0].id,
        "seeded-in",
        "a full cache is left alone"
    );
    assert_eq!(output.borrow()[0].id, "seeded-out");
}

#[test]
fn an_empty_cache_is_filled_from_the_host() {
    let input = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::new(RefCell::new(Vec::new()));
    ensure_devices_loaded(&input, &output);
    // On a machine with no interfaces this legitimately stays empty; what must
    // hold is that it matches what the host reports right now.
    let m = model();
    assert_eq!(input.borrow().len(), refresh_input_devices(&m).len());
    assert_eq!(output.borrow().len(), refresh_output_devices(&m).len());
}
