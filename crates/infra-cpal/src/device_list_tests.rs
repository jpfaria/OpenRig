//! #1081 — nothing in this crate asks cpal for its filtered device lists or a
//! device's description: on CoreAudio both build an AudioUnit per device,
//! and every new AudioUnit starts and stops an IOProc on the default output
//! (the HD 8) inside OpenRig's own IO context. The device list is re-read
//! every 10 s while chains play, so one such call in the scan is enough to
//! touch the running HD 8 forever.

use std::fs;
use std::path::Path;

/// The production sources of this crate, as `(file name, text)`.
fn production_sources() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    let mut dirs = vec![dir];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if name.ends_with(".rs") && !name.ends_with("_tests.rs") {
                sources.push((name, fs::read_to_string(&path).unwrap()));
            }
        }
    }
    sources
}

fn files_calling(call: &str, allowed: &str) -> Vec<String> {
    let mut files: Vec<String> = production_sources()
        .into_iter()
        .filter(|(name, text)| name != allowed && text.contains(call))
        .map(|(name, _)| name)
        .collect();
    files.sort();
    files
}

#[test]
fn only_the_device_list_asks_cpal_for_input_devices() {
    assert_eq!(
        files_calling(".input_devices()", "device_list.rs"),
        Vec::<String>::new()
    );
}

#[test]
fn only_the_device_list_asks_cpal_for_output_devices() {
    assert_eq!(
        files_calling(".output_devices()", "device_list.rs"),
        Vec::<String>::new()
    );
}

#[test]
fn only_the_name_cache_asks_a_device_for_its_description() {
    assert_eq!(
        files_calling(".description()", "device_name_cache.rs"),
        Vec::<String>::new()
    );
}
