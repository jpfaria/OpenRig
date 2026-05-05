//! End-to-end: a disk-backed plugin package must surface its parameters
//! through `project::block::schema_for_block_model` so the GUI can render
//! knobs. Issue #287.
//!
//! Integration test (own binary). All assertions live inside one
//! `#[test]` because `plugin_loader::registry` is `OnceLock`-backed —
//! the first `init` call freezes the catalog for the rest of the
//! process. Running multiple `#[test]` fns in parallel would race.

use std::fs;
use std::path::{Path, PathBuf};

fn tmp_root(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "openrig-disk-pkg-schema-{label}-{}",
        std::process::id()
    ));
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
fn disk_packages_synthesize_schema_parameters_from_manifest() {
    let root = tmp_root("synth");

    // NAM with two numeric grid axes.
    let nam = root.join("nam_test_amp");
    write(
        &nam.join("manifest.yaml"),
        br#"manifest_version: 1
id: nam_test_amp_e2e
display_name: Test Amp
brand: testco
type: amp
backend: nam
parameters:
  - name: gain
    display_name: Gain
    values: [10.0, 20.0, 30.0]
  - name: volume
    values: [50.0, 60.0]
captures:
  - values: { gain: 10.0, volume: 50.0 }
    file: g10v50.nam
  - values: { gain: 20.0, volume: 50.0 }
    file: g20v50.nam
  - values: { gain: 30.0, volume: 60.0 }
    file: g30v60.nam
"#,
    );
    write(&nam.join("g10v50.nam"), b"fake");
    write(&nam.join("g20v50.nam"), b"fake");
    write(&nam.join("g30v60.nam"), b"fake");

    // IR with a single text axis (enum-style picker).
    let ir = root.join("ir_test_cab");
    write(
        &ir.join("manifest.yaml"),
        br#"manifest_version: 1
id: ir_test_cab_e2e
display_name: Test Cab
brand: testco
type: cab
backend: ir
parameters:
  - name: voicing
    display_name: Voicing
    values: [bright, dark, neutral]
captures:
  - values: { voicing: bright }
    file: bright.wav
  - values: { voicing: dark }
    file: dark.wav
  - values: { voicing: neutral }
    file: neutral.wav
"#,
    );
    write(&ir.join("bright.wav"), b"fake");
    write(&ir.join("dark.wav"), b"fake");
    write(&ir.join("neutral.wav"), b"fake");

    // LV2 with shared data/ layout (TTLs deduped, binary per-slot).
    let lv2 = root.join("lv2_test_chorus");
    write(
        &lv2.join("manifest.yaml"),
        br#"manifest_version: 1
id: lv2_test_chorus_e2e
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
        &lv2.join("data").join("manifest.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:chorus> a lv2:Plugin ; lv2:binary <chorus.so> .\n",
    );
    write(
        &lv2.join("data").join("chorus.ttl"),
        b"@prefix lv2: <http://lv2plug.in/ns/lv2core#> .\n\
<urn:test:chorus>\n\
    a lv2:Plugin ;\n\
    lv2:port [ a lv2:InputPort, lv2:AudioPort ; lv2:index 0 ; lv2:symbol \"in\" ; ] ,\n\
    [ a lv2:OutputPort, lv2:AudioPort ; lv2:index 1 ; lv2:symbol \"out\" ; ] ,\n\
    [ a lv2:InputPort, lv2:ControlPort ; lv2:index 2 ; lv2:symbol \"rate\" ; lv2:default 1.5 ; lv2:minimum 0.1 ; lv2:maximum 10.0 ; ] ,\n\
    [ a lv2:InputPort, lv2:ControlPort ; lv2:index 3 ; lv2:symbol \"depth\" ; lv2:default 50.0 ; lv2:minimum 0.0 ; lv2:maximum 100.0 ; ] .\n",
    );
    write(
        &lv2.join("platform")
            .join("macos-universal")
            .join("chorus.dylib"),
        b"fake-bin",
    );

    // ALL fixtures must exist before init() — OnceLock freezes the
    // catalog on the first call.
    plugin_loader::registry::init(&root);

    // ── NAM: two numeric grid axes → two float parameters ──
    let amp_schema = project::block::schema_for_block_model("amp", "nam_test_amp_e2e")
        .expect("NAM schema should resolve via plugin_loader fallback");
    let amp_names: Vec<&str> = amp_schema
        .parameters
        .iter()
        .map(|p| p.path.as_str())
        .collect();
    assert!(
        amp_names.contains(&"gain"),
        "NAM schema missing 'gain', got: {amp_names:?}"
    );
    assert!(
        amp_names.contains(&"volume"),
        "NAM schema missing 'volume', got: {amp_names:?}"
    );
    assert_eq!(amp_schema.parameters.len(), 2);

    // ── IR: one text axis → one enum parameter ──
    let cab_schema = project::block::schema_for_block_model("cab", "ir_test_cab_e2e")
        .expect("IR schema should resolve");
    let cab_names: Vec<&str> = cab_schema
        .parameters
        .iter()
        .map(|p| p.path.as_str())
        .collect();
    assert!(
        cab_names.contains(&"voicing"),
        "IR schema missing 'voicing', got: {cab_names:?}"
    );
    assert_eq!(cab_schema.parameters.len(), 1);

    // ── LV2: data/ TTLs + per-platform binary → control ports become float params ──
    let lv2_schema = project::block::schema_for_block_model("modulation", "lv2_test_chorus_e2e")
        .expect("LV2 schema should resolve via shared data/ TTLs");
    let lv2_names: Vec<&str> = lv2_schema
        .parameters
        .iter()
        .map(|p| p.path.as_str())
        .collect();
    assert!(
        lv2_names.contains(&"rate") && lv2_names.contains(&"depth"),
        "LV2 schema should include `rate` and `depth` ControlIn ports from data/chorus.ttl, got: {lv2_names:?}"
    );
}
