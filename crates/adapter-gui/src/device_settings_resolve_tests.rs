//! The settings a device is opened with, resolved from what the user saved.
//!
//! Pure: no device, no window. An unsupported value in `gui-settings.yaml`
//! (hand-edited, or written by an older build) must fall back to the default
//! rather than reach the audio layer, and a blank field in the picker must be
//! a named error, not a silent zero.

use super::{default_device_settings, normalize_device_settings, selected_device_settings};
use crate::{
    DeviceSelectionItem, DEFAULT_BIT_DEPTH, DEFAULT_BUFFER_SIZE_FRAMES, DEFAULT_SAMPLE_RATE,
};
use infra_filesystem::GuiAudioDeviceSettings;
use slint::VecModel;
use std::rc::Rc;

fn row(selected: bool, rate: &str, buffer: &str, depth: &str) -> DeviceSelectionItem {
    DeviceSelectionItem {
        device_id: "dev".into(),
        name: "Interface".into(),
        selected,
        sample_rate_text: rate.into(),
        buffer_size_text: buffer.into(),
        bit_depth_text: depth.into(),
    }
}

fn saved(rate: u32, buffer: u32, depth: u32) -> GuiAudioDeviceSettings {
    let mut s = default_device_settings("dev".into(), "Interface".into());
    s.sample_rate = rate;
    s.buffer_size_frames = buffer;
    s.bit_depth = depth;
    s
}

#[test]
fn a_supported_setting_survives_normalization() {
    let kept = normalize_device_settings(saved(48_000, 128, 32));

    assert_eq!(kept.sample_rate, 48_000);
    assert_eq!(kept.buffer_size_frames, 128);
    assert_eq!(kept.bit_depth, 32);
}

#[test]
fn an_unsupported_setting_falls_back_to_the_default() {
    let fixed = normalize_device_settings(saved(1, 7, 3));

    assert_eq!(fixed.sample_rate, DEFAULT_SAMPLE_RATE);
    assert_eq!(fixed.buffer_size_frames, DEFAULT_BUFFER_SIZE_FRAMES);
    assert_eq!(fixed.bit_depth, DEFAULT_BIT_DEPTH);
}

#[test]
fn only_the_selected_rows_are_collected() {
    let model = Rc::new(VecModel::from(vec![
        row(true, "48000", "128", "32"),
        row(false, "44100", "64", "24"),
    ]));

    let picked = selected_device_settings(&model, "input").expect("valid rows");

    assert_eq!(picked.len(), 1, "an unselected device is not opened");
    assert_eq!(picked[0].sample_rate, 48_000);
    assert_eq!(picked[0].buffer_size_frames, 128);
}

#[test]
fn a_blank_field_is_a_named_error_not_a_zero() {
    let model = Rc::new(VecModel::from(vec![row(true, "", "128", "32")]));

    let err = selected_device_settings(&model, "input")
        .expect_err("a blank sample rate cannot resolve")
        .to_string();

    assert!(
        err.contains("input_sample_rate") && err.contains("Interface"),
        "the error names the field and the device — got {err}"
    );
}

#[test]
fn no_selected_device_resolves_to_an_empty_list() {
    let model = Rc::new(VecModel::from(vec![row(false, "48000", "128", "32")]));

    assert!(selected_device_settings(&model, "output")
        .expect("no row to parse")
        .is_empty());
}
