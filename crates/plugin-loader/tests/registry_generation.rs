//! The catalog carries a generation that moves on every edit, so a reader
//! can keep what it derived from the catalog until the catalog changes.

use std::fs;
use std::path::Path;

use plugin_loader::registry::{generation, load_one, reload, unload};

fn write_ir_plugin(parent: &Path, id: &str) {
    let dir = parent.join(id);
    fs::create_dir_all(&dir).expect("create plugin dir");
    fs::write(dir.join("imp.wav"), b"").expect("write placeholder wav");
    let manifest = format!(
        "manifest_version: 1\n\
         id: {id}\n\
         display_name: Generation Cab\n\
         type: cab\n\
         backend: ir\n\
         parameters:\n\
         - name: capture\n  \
           display_name: Capture\n  \
           values:\n  \
           - main\n\
         captures:\n\
         - values:\n    \
             capture: main\n  \
           file: imp.wav\n"
    );
    fs::write(dir.join("manifest.yaml"), manifest).expect("write manifest");
}

#[test]
fn every_catalog_edit_moves_the_generation() {
    let parent = std::env::temp_dir().join(format!(
        "openrig_registry_generation_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&parent);
    write_ir_plugin(&parent, "generation_cab");

    let start = generation();
    reload(std::slice::from_ref(&parent));
    let reloaded = generation();
    assert!(reloaded > start, "reload kept generation {start}");

    unload("generation_cab").expect("unload");
    let unloaded = generation();
    assert!(unloaded > reloaded, "unload kept generation {reloaded}");

    load_one("generation_cab", std::slice::from_ref(&parent)).expect("load_one");
    assert!(
        generation() > unloaded,
        "load_one kept generation {unloaded}"
    );

    let _ = fs::remove_dir_all(&parent);
}
