//! LV2 control ports without `lv2:portProperty lv2:integer`/`enumeration`
//! are continuous knobs — their default values are arbitrary floats
//! and must NOT trip the schema's step-alignment check. Issue #287.
//!
//! Reproduced by tap_chorus_flanger's "Contour" port: range 20-20000,
//! default 100. The synthesized step was (max-min)/100 = 199.8, so
//! validating default=100 failed with "value 100 does not align with
//! step 199.8" the moment the user toggled the block's power button.

use std::fs;
use std::path::{Path, PathBuf};

fn tmp_root(label: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("openrig-lv2-cont-{label}-{}", std::process::id()));
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

#[test]
fn continuous_lv2_default_validates_against_synthesized_schema() {
    let root = tmp_root("contour");

    let pkg = root.join("lv2_test_chorus");
    write(
        &pkg.join("manifest.yaml"),
        br#"manifest_version: 1
id: lv2_test_chorus
display_name: Test Chorus
brand: testco
type: mod
backend: lv2
plugin_uri: urn:test:chorus
binaries:
  macos-universal: platform/macos-universal/chorus.dylib
"#,
    );
    write(
        &pkg.join("data").join("manifest.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
          <urn:test:chorus> a lv2:Plugin ; lv2:binary <chorus.so> .\n",
    );
    write(
        &pkg.join("data").join("chorus.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
          <urn:test:chorus>\n\
              a lv2:Plugin ;\n\
              lv2:port [ a lv2:InputPort, lv2:AudioPort ; lv2:index 0 ; lv2:symbol \"in\" ] ,\n\
              [ a lv2:OutputPort, lv2:AudioPort ; lv2:index 1 ; lv2:symbol \"out\" ] ,\n\
              [ a lv2:InputPort, lv2:ControlPort ;\n\
                lv2:index 2 ;\n\
                lv2:symbol \"Contour\" ;\n\
                lv2:default 100.0 ;\n\
                lv2:minimum 20.0 ;\n\
                lv2:maximum 20000.0 ;\n\
              ] .\n",
    );
    write(
        &pkg.join("platform")
            .join("macos-universal")
            .join("chorus.dylib"),
        b"fake-bin",
    );

    plugin_loader::registry::init(&root);

    let schema = project::block::schema_for_block_model("modulation", "lv2_test_chorus")
        .expect("schema must resolve via plugin_loader fallback");

    let contour = schema
        .parameters
        .iter()
        .find(|p| p.path == "Contour")
        .expect("schema must include Contour parameter");

    let default = contour
        .default_value
        .clone()
        .expect("Contour must have a default value from TTL");

    // The default value lives in the schema; it MUST validate against
    // the schema's own constraints. If it fails, every block load that
    // uses defaults breaks.
    contour
        .validate_value(&default)
        .expect("default value must validate against the synthesized schema");
}
