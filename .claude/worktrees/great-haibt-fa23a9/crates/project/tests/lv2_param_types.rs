//! LV2 schema synthesis must respect `lv2:portProperty lv2:toggled`,
//! `lv2:enumeration`, and `lv2:integer` so the GUI gets checkboxes /
//! dropdowns / steppers instead of generic float sliders. Issue #401.
//!
//! Reproduced from x42 fat1 autotune: 12 toggles `m00..m11` (one per
//! note in the chromatic scale) plus a `mode` enum (Auto/MIDI/Manual).
//!
//! `plugin_loader::registry` is OnceLock-backed, so all packages this
//! file exercises are registered into a single shared root and the
//! registry is initialised exactly once.

use std::fs;
use std::path::{Path, PathBuf};

fn shared_root() -> PathBuf {
    let path = std::env::temp_dir().join(format!("openrig-lv2-params-{}", std::process::id()));
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

fn make_pitch_pkg_with_toggle(root: &Path) {
    let pkg = root.join("lv2_test_pitch_toggle");
    write(
        &pkg.join("manifest.yaml"),
        br#"manifest_version: 1
id: lv2_test_pitch_toggle
display_name: Test Pitch Toggle
brand: testco
type: pitch
backend: lv2
plugin_uri: urn:test:pitch_toggle
binaries:
  macos-universal: platform/macos-universal/pitch.dylib
"#,
    );
    write(
        &pkg.join("data").join("manifest.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
          <urn:test:pitch_toggle> a lv2:Plugin ; lv2:binary <pitch.so> .\n",
    );
    write(
        &pkg.join("data").join("pitch.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
          <urn:test:pitch_toggle>\n\
              a lv2:Plugin ;\n\
              lv2:port [ a lv2:InputPort, lv2:AudioPort ; lv2:index 0 ; lv2:symbol \"in\" ] ,\n\
              [ a lv2:OutputPort, lv2:AudioPort ; lv2:index 1 ; lv2:symbol \"out\" ] ,\n\
              [ a lv2:InputPort, lv2:ControlPort ;\n\
                lv2:index 2 ;\n\
                lv2:symbol \"note_c\" ;\n\
                lv2:name \"C\" ;\n\
                lv2:default 1 ;\n\
                lv2:minimum 0 ;\n\
                lv2:maximum 1 ;\n\
                lv2:portProperty lv2:integer, lv2:toggled ;\n\
              ] .\n",
    );
    write(
        &pkg.join("platform")
            .join("macos-universal")
            .join("pitch.dylib"),
        b"fake-bin",
    );
}

fn make_pitch_pkg_with_enum(root: &Path) {
    let pkg = root.join("lv2_test_pitch_enum");
    write(
        &pkg.join("manifest.yaml"),
        br#"manifest_version: 1
id: lv2_test_pitch_enum
display_name: Test Pitch Enum
brand: testco
type: pitch
backend: lv2
plugin_uri: urn:test:pitch_enum
binaries:
  macos-universal: platform/macos-universal/pitchenum.dylib
"#,
    );
    write(
        &pkg.join("data").join("manifest.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
          <urn:test:pitch_enum> a lv2:Plugin ; lv2:binary <pitchenum.so> .\n",
    );
    write(
        &pkg.join("data").join("pitchenum.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
          @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
          @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
          <urn:test:pitch_enum>\n\
              a lv2:Plugin ;\n\
              lv2:port [ a lv2:InputPort, lv2:AudioPort ; lv2:index 0 ; lv2:symbol \"in\" ] ,\n\
              [ a lv2:OutputPort, lv2:AudioPort ; lv2:index 1 ; lv2:symbol \"out\" ] ,\n\
              [ a lv2:InputPort, lv2:ControlPort ;\n\
                lv2:index 2 ;\n\
                lv2:symbol \"mode\" ;\n\
                lv2:name \"Mode\" ;\n\
                lv2:default 0 ;\n\
                lv2:minimum 0 ;\n\
                lv2:maximum 2 ;\n\
                lv2:portProperty lv2:integer, lv2:enumeration ;\n\
                lv2:scalePoint [ rdfs:label \"Auto\" ; rdf:value 0 ; ] ;\n\
                lv2:scalePoint [ rdfs:label \"MIDI\" ; rdf:value 1 ; ] ;\n\
                lv2:scalePoint [ rdfs:label \"Manual\" ; rdf:value 2 ; ] ;\n\
              ] .\n",
    );
    write(
        &pkg.join("platform")
            .join("macos-universal")
            .join("pitchenum.dylib"),
        b"fake-bin",
    );
}

/// One-shot setup. The OnceLock-backed registry only honours the very
/// first `init` call, so every test in this file must hit a registry
/// that already saw both packages.
fn shared_registry() -> &'static PathBuf {
    use std::sync::OnceLock;
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = shared_root();
        make_pitch_pkg_with_toggle(&root);
        make_pitch_pkg_with_enum(&root);
        plugin_loader::registry::init(&root);
        root
    })
}

#[test]
fn synthesized_schema_routes_lv2_toggled_to_bool_parameter() {
    let _ = shared_registry();

    let schema = project::block::schema_for_block_model("pitch", "lv2_test_pitch_toggle")
        .expect("schema resolves via plugin_loader fallback");

    let note = schema
        .parameters
        .iter()
        .find(|p| p.path == "note_c")
        .expect("note_c parameter present");

    use block_core::param::{ParameterDomain, ParameterWidget};
    assert!(
        matches!(note.widget, ParameterWidget::Toggle),
        "expected Toggle widget for `lv2:toggled` port, got {:?}",
        note.widget
    );
    assert!(
        matches!(note.domain, ParameterDomain::Bool),
        "expected Bool domain for `lv2:toggled` port, got {:?}",
        note.domain
    );
    assert_eq!(note.label, "C");
}

#[test]
fn synthesized_schema_routes_enumeration_with_scale_points_to_enum_parameter() {
    let _ = shared_registry();

    let schema = project::block::schema_for_block_model("pitch", "lv2_test_pitch_enum")
        .expect("schema resolves via plugin_loader fallback");

    let mode = schema
        .parameters
        .iter()
        .find(|p| p.path == "mode")
        .expect("mode parameter present");

    use block_core::param::{ParameterDomain, ParameterWidget};
    assert!(
        matches!(mode.widget, ParameterWidget::Select),
        "expected Select widget for `lv2:enumeration` port, got {:?}",
        mode.widget
    );
    let ParameterDomain::Enum { options } = &mode.domain else {
        panic!("expected Enum domain, got {:?}", mode.domain);
    };
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].label, "Auto");
    assert_eq!(options[1].label, "MIDI");
    assert_eq!(options[2].label, "Manual");
}
