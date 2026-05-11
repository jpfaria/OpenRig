//! Body must use the rich panel editor like every other native block.
//! Issue #287.
//!
//! Integration test: `plugin_loader::registry` is `OnceLock`-backed —
//! the first init call freezes the catalog for the rest of the
//! process. We register a single fake body package up-front so that
//! `supported_block_types()` sees disk packages for the `body` type.

use std::fs;
use std::path::{Path, PathBuf};

fn tmp_root(label: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("openrig-body-panel-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create tmp root");
    path
}

fn write(path: &Path, contents: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, contents).expect("write file");
}

fn init_with_body_package() {
    let root = tmp_root("init");
    let pkg = root.join("ir_body_test");
    write(
        &pkg.join("manifest.yaml"),
        br#"manifest_version: 1
id: ir_body_test
display_name: Body Test
brand: testco
type: body
backend: ir
parameters:
  - name: voicing
    display_name: Voicing
    values: [bright, neutral]
captures:
  - values: { voicing: bright }
    file: bright.wav
  - values: { voicing: neutral }
    file: neutral.wav
"#,
    );
    write(&pkg.join("bright.wav"), b"fake");
    write(&pkg.join("neutral.wav"), b"fake");
    plugin_loader::registry::init(&root);
}

#[test]
fn supported_block_type_body_uses_panel_editor() {
    init_with_body_package();
    let entry = project::catalog::supported_block_type("body")
        .expect("body type must be registered in the block catalog");
    assert!(
        entry.use_panel_editor,
        "body must use the rich panel editor like preamp/amp/cab/mod; got use_panel_editor=false"
    );
}

#[test]
fn supported_block_types_list_marks_body_as_panel_editor() {
    init_with_body_package();
    let body = project::catalog::supported_block_types()
        .into_iter()
        .find(|e| e.effect_type == "body")
        .expect("body must appear in supported_block_types() once disk packages exist");
    assert!(
        body.use_panel_editor,
        "body must use the rich panel editor in the catalog list; got use_panel_editor=false"
    );
}
