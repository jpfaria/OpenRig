//! Issue #599 — IR and NAM block tiles missing from block-type-picker.
//!
//! Root cause: plugin catalog not loaded because config.yaml uses
//! `paths.plugins_path` but the loader only checks top-level `plugins_root`.
//! Result: catalog reports 0 disk packages → IR/NAM types never appear
//! in the block_type_picker_items() output.
//!
//! Red-first: this test asserts that IR and NAM block types ARE included
//! in the picker when the plugin catalog has at least one IR/NAM package
//! loaded from disk.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use adapter_gui::project_view::block_type_picker_items;
use adapter_gui::BlockTypePickerItem;

/// HOME is process-global; serialize tests that swap it.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp = std::env::temp_dir()
        .join(format!("openrig-599-{label}-{}-{now}", std::process::id()));
    std::fs::create_dir_all(&tmp).expect("mkdir tempdir");
    let prev = std::env::var_os("HOME");
    std::env::set_var("HOME", &tmp);
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&tmp)));
    if let Some(prev) = prev {
        std::env::set_var("HOME", prev);
    } else {
        std::env::remove_var("HOME");
    }
    let _ = std::fs::remove_dir_all(&tmp);
    res.unwrap_or_else(|e| std::panic::resume_unwind(e));
}

#[test]
fn block_type_picker_includes_ir_when_catalog_has_ir_packages() {
    with_temp_home("block-picker-ir", |_home| {
        // When the plugin catalog is loaded with at least one IR package,
        // block_type_picker_items() must include an IR block type.
        let items = block_type_picker_items("test_instrument");

        let has_ir = items.iter().any(|item| {
            // The IR tile should be present. Exact label TBD from production code.
            item.title.to_lowercase().contains("ir")
                || item.title.to_lowercase().contains("impulse")
        });

        assert!(
            has_ir,
            "block_type_picker_items did not include IR type. Items: {:?}",
            items.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    });
}

#[test]
fn block_type_picker_includes_nam_when_catalog_has_nam_packages() {
    with_temp_home("block-picker-nam", |_home| {
        // When the plugin catalog is loaded with at least one NAM package,
        // block_type_picker_items() must include a NAM block type.
        let items = block_type_picker_items("test_instrument");

        let has_nam = items.iter().any(|item| {
            // The NAM tile should be present.
            item.title.to_lowercase().contains("nam")
                || item.title.to_lowercase().contains("neural")
        });

        assert!(
            has_nam,
            "block_type_picker_items did not include NAM type. Items: {:?}",
            items.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    });
}

#[test]
fn block_type_picker_includes_both_ir_and_nam_when_catalog_loaded() {
    with_temp_home("block-picker-both", |_home| {
        // When catalog has both IR and NAM packages, picker includes both.
        let items = block_type_picker_items("test_instrument");

        let ir_count = items
            .iter()
            .filter(|item| item.title.to_lowercase().contains("ir"))
            .count();
        let nam_count = items
            .iter()
            .filter(|item| item.title.to_lowercase().contains("nam"))
            .count();

        assert_eq!(ir_count, 1, "Expected exactly 1 IR tile; found {}", ir_count);
        assert_eq!(
            nam_count, 1,
            "Expected exactly 1 NAM tile; found {}",
            nam_count
        );
    });
}

#[test]
fn block_type_picker_native_types_always_present() {
    with_temp_home("block-picker-native", |_home| {
        // Native types (PREAMP, AMP, CAB, etc.) must ALWAYS be in the picker,
        // regardless of whether the catalog is loaded.
        let items = block_type_picker_items("test_instrument");

        let native_type_labels = vec![
            "preamp", "amp", "cab", "gain", "dyn", "filter", "wah", "pitch", "mod", "dly",
            "rvb",
        ];

        for expected_label in native_type_labels {
            let found = items.iter().any(|item| {
                item.title.to_lowercase().contains(expected_label)
            });
            assert!(
                found,
                "Native type '{}' not found in picker. Items: {:?}",
                expected_label,
                items.iter().map(|i| &i.title).collect::<Vec<_>>()
            );
        }
    });
}
