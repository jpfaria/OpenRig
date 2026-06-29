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

use std::path::PathBuf;
use std::sync::Mutex;

use adapter_gui::project_view::block_type_picker_items;

/// HOME is process-global; serialize tests that swap it.
static HOME_LOCK: Mutex<()> = Mutex::new(());

fn with_temp_home<F: FnOnce(&PathBuf)>(label: &str, f: F) {
    let _g = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let tmp =
        std::env::temp_dir().join(format!("openrig-599-{label}-{}-{now}", std::process::id()));
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
        // The IR block type is offered for an electric-guitar chain.
        let items = block_type_picker_items("electric_guitar");

        let has_ir = items.iter().any(|item| item.effect_type.as_str() == "ir");

        assert!(
            has_ir,
            "block_type_picker_items did not include IR type. Items: {:?}",
            items.iter().map(|i| &i.effect_type).collect::<Vec<_>>()
        );
    });
}

#[test]
fn block_type_picker_includes_nam_when_catalog_has_nam_packages() {
    with_temp_home("block-picker-nam", |_home| {
        // The NAM block type is offered for an electric-guitar chain.
        let items = block_type_picker_items("electric_guitar");

        let has_nam = items.iter().any(|item| item.effect_type.as_str() == "nam");

        assert!(
            has_nam,
            "block_type_picker_items did not include NAM type. Items: {:?}",
            items.iter().map(|i| &i.effect_type).collect::<Vec<_>>()
        );
    });
}

#[test]
fn block_type_picker_includes_both_ir_and_nam_when_catalog_loaded() {
    with_temp_home("block-picker-both", |_home| {
        // The picker offers exactly one IR tile and one NAM tile.
        let items = block_type_picker_items("electric_guitar");

        let ir_count = items
            .iter()
            .filter(|item| item.effect_type.as_str() == "ir")
            .count();
        let nam_count = items
            .iter()
            .filter(|item| item.effect_type.as_str() == "nam")
            .count();

        assert_eq!(
            ir_count, 1,
            "Expected exactly 1 IR tile; found {}",
            ir_count
        );
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
        let items = block_type_picker_items("electric_guitar");

        let native_type_ids = vec![
            "preamp",
            "amp",
            "cab",
            "gain",
            "dynamics",
            "filter",
            "wah",
            "pitch",
            "modulation",
            "delay",
            "reverb",
        ];

        for expected_id in native_type_ids {
            let found = items
                .iter()
                .any(|item| item.effect_type.as_str() == expected_id);
            assert!(
                found,
                "Native type '{}' not found in picker. Items: {:?}",
                expected_id,
                items.iter().map(|i| &i.effect_type).collect::<Vec<_>>()
            );
        }
    });
}
