//! #986 — the owner's decision: the project is written ONLY on an explicit
//! save. There is no autosave mode at all, not even behind a flag: the
//! `--auto-save` CLI flag, the `OPENRIG_AUTO_SAVE` env var and every
//! `auto_save` switch in the GUI wiring are gone.
//!
//! Which frontend path writes the file is a wiring question an `AppWindow`
//! test cannot see, so this follows the crate's source-presence convention
//! (see `no_native_dialogs.rs`): any production source that still carries an
//! autosave switch fails here. The behavioral side (edits leave the file
//! untouched, `SaveProject` writes it) is `src/issue_986_no_autosave_tests.rs`.

use std::path::{Path, PathBuf};

fn production_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            production_sources(&path, out);
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let is_source = name.ends_with(".rs") || name.ends_with(".slint");
        if is_source && !name.contains("test") {
            out.push(path);
        }
    }
}

#[test]
fn no_production_source_carries_an_autosave_switch() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    production_sources(&root.join("src"), &mut files);
    production_sources(&root.join("ui"), &mut files);
    assert!(!files.is_empty(), "found no production sources to scan");

    let needles = ["auto_save", "auto-save", "AUTO_SAVE", "autosave"];
    let offenders: Vec<String> = files
        .iter()
        .filter_map(|f| {
            let src = std::fs::read_to_string(f).ok()?;
            let hits: Vec<&str> = needles
                .iter()
                .copied()
                .filter(|n| src.contains(n))
                .collect();
            (!hits.is_empty()).then(|| {
                format!(
                    "{} ({})",
                    f.strip_prefix(&root).unwrap().display(),
                    hits.join(", ")
                )
            })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "#986: no autosave — the project is written only on an explicit save; \
         these production sources still carry an autosave switch: {offenders:#?}"
    );
}
