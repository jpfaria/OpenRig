use super::*;
use std::path::Path;

#[test]
fn preset_filename_appends_yaml_extension() {
    assert_eq!(preset_filename("Clean strum"), "Clean strum.yaml");
}

#[test]
fn preset_filename_preserves_unicode_and_spaces() {
    assert_eq!(
        preset_filename("Clocks — Coldplay (rhythm)"),
        "Clocks — Coldplay (rhythm).yaml"
    );
}

#[test]
fn preset_filename_sanitises_illegal_chars() {
    assert_eq!(preset_filename("a/b\\c:d*e?f"), "a_b_c_d_e_f.yaml");
}

#[test]
fn preset_save_path_joins_under_presets_dir() {
    let dir = Path::new("/tmp/openrig/presets");
    assert_eq!(
        preset_save_path(dir, "lead"),
        Path::new("/tmp/openrig/presets/lead.yaml")
    );
}

// #978: Windows reserves these device names whatever the extension, so
// `AUX.yaml` is the AUX device, not a file: the save failed.
#[test]
fn windows_reserved_names_get_a_suffix_on_windows() {
    for name in ["CON", "prn", "Aux", "NUL", "com1", "LPT9"] {
        assert_eq!(
            windows_safe_stem(name, true),
            format!("{name}_"),
            "{name} is a device on Windows"
        );
    }
}

#[test]
fn names_that_only_start_like_a_device_are_kept() {
    for name in ["Aux lead", "Console", "COM10", "Null", "LPT"] {
        assert_eq!(windows_safe_stem(name, true), name);
    }
}

#[test]
fn reserved_names_are_kept_off_windows() {
    // The cross-platform rule: a Windows fix stays on Windows.
    assert_eq!(windows_safe_stem("AUX", false), "AUX");
}

// Windows reads the device name up to the first '.', trailing spaces
// dropped: "AUX.clean" and "NUL.v2" are the devices too. The suffix goes
// right after the device part, or the name would still be reserved.
#[test]
fn a_reserved_name_before_a_dot_is_still_a_device() {
    assert_eq!(windows_safe_stem("Aux.clean", true), "Aux_.clean");
    assert_eq!(windows_safe_stem("nul.v2", true), "nul_.v2");
    assert_eq!(windows_safe_stem("Con .lead", true), "Con_ .lead");
}

#[test]
fn port_zero_and_superscript_ports_are_devices_too() {
    for name in ["COM0", "lpt0", "COM¹", "LPT³"] {
        assert_eq!(windows_safe_stem(name, true), format!("{name}_"));
    }
}

#[test]
fn a_dotted_name_that_is_not_a_device_is_kept() {
    assert_eq!(
        windows_safe_stem("Auxiliary.clean", true),
        "Auxiliary.clean"
    );
    assert_eq!(windows_safe_stem("v1.AUX", true), "v1.AUX");
}
