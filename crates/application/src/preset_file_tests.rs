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
