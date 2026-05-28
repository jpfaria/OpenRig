//! Issue #581 — In Settings → System → Audio, clicking the Hz / Buffer /
//! Bits ComboBox of a selected device deselects the device instead of
//! opening the dropdown.
//!
//! Root cause (in `section_system_audio.slint`): the toggle `TouchArea`
//! is the last sibling inside `AudioDeviceRow` (top of the Slint
//! z-order) with a hard-coded `width: 300px`. The selected-state
//! controls are positioned relative to the right edge — the sample-rate
//! ComboBox at `x: parent.width - 394px`. Whenever the row is narrower
//! than ~694px the ComboBox lives inside the TouchArea bounding box and
//! the toggle wins the hit test, firing `toggled(!device.selected)`.
//!
//! The Rust backend (`update_device_sample_rate` in
//! `crates/adapter-gui/src/audio_devices.rs`) is correct and already
//! preserves `selected`. The bug is purely in the Slint layout, so this
//! test is a static invariant on the `.slint` source: the `TouchArea`
//! width must be conditional on `root.device.selected` so it can shrink
//! to leave room for the controls when the row is expanded. A literal
//! `width: 300px;` (or any other fixed width) is forbidden.

use std::fs;
use std::path::PathBuf;

fn slint_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = manifest_dir
        .join("ui")
        .join("pages")
        .join("settings")
        .join("section_system_audio.slint");
    fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Extract the `TouchArea { ... }` block that lives inside
/// `component AudioDeviceRow inherits Rectangle { ... }`. We scan for
/// the literal `TouchArea {` and return the brace-matched body.
fn toggle_touch_area_body(source: &str) -> String {
    let component_marker = "component AudioDeviceRow inherits Rectangle {";
    let component_start = source
        .find(component_marker)
        .expect("component AudioDeviceRow not found in section_system_audio.slint");

    let after_component = &source[component_start..];
    let touch_area_marker = "TouchArea {";
    let touch_area_rel = after_component
        .find(touch_area_marker)
        .expect("TouchArea not found inside AudioDeviceRow");
    let body_start = component_start + touch_area_rel + touch_area_marker.len();

    let bytes = source.as_bytes();
    let mut depth = 1i32;
    let mut i = body_start;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return source[body_start..i].to_string();
        }
        i += 1;
    }
    panic!("unterminated TouchArea block in AudioDeviceRow");
}

/// Extract the value bound to `width:` inside the given block. Returns
/// the substring up to (but not including) the terminating `;`.
fn width_expression(block: &str) -> String {
    let marker = "width:";
    let start = block
        .find(marker)
        .expect("TouchArea has no `width:` binding");
    let after_marker = &block[start + marker.len()..];
    let end = after_marker
        .find(';')
        .expect("`width:` binding is missing a terminating `;`");
    after_marker[..end].trim().to_string()
}

#[test]
fn issue_581_toggle_touch_area_width_is_conditional_on_selected() {
    let source = slint_source();
    let body = toggle_touch_area_body(&source);
    let width = width_expression(&body);

    assert!(
        width.contains("root.device.selected"),
        "AudioDeviceRow's toggle TouchArea has `width: {width};` — a width that does NOT \
         depend on `root.device.selected` will cover the Hz/Buffer/Bits ComboBoxes when the \
         row expands (sample-rate ComboBox starts at `x: parent.width - 394px`), stealing \
         their clicks and deselecting the device. Make the width conditional, e.g. \
         `width: root.device.selected ? parent.width - 424px : parent.width;`."
    );
}
