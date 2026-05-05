//! Disk-package schemas must declare an `audio_mode` that matches the
//! backend's actual channel topology. Hardcoding DualMono for every
//! disk-backed plugin causes NAM amps (mono-only DSP) to be
//! instantiated TWICE per chain (one for L, one for R), and the NAM
//! C SDK doesn't tolerate two concurrent instances of the same model
//! cleanly — the runtime howls/feedbacks when the block is enabled.
//! Issue #287, reproduced live as "microfonia ao ativar mesa rectifier".

use std::fs;
use std::path::{Path, PathBuf};

fn tmp_root(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "openrig-pkg-audio-mode-{label}-{}",
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
fn disk_package_schemas_use_per_backend_audio_mode() {
    let root = tmp_root("modes");

    // NAM amp — mono in / mono out. Must be MonoOnly so the engine
    // builds a single NAMProcessor and broadcasts to stereo, instead
    // of creating two concurrent C SDK instances of the same model.
    let nam = root.join("nam_test_amp");
    write(
        &nam.join("manifest.yaml"),
        br#"manifest_version: 1
id: nam_audio_mode_test
display_name: NAM Test
brand: testco
type: amp
backend: nam
parameters:
  - name: gain
    values: [low, high]
captures:
  - values: { gain: low }
    file: low.nam
  - values: { gain: high }
    file: high.nam
"#,
    );
    write(&nam.join("low.nam"), b"fake");
    write(&nam.join("high.nam"), b"fake");

    // IR cab — mono convolution, mono in / mono out. Must be MonoOnly.
    let ir = root.join("ir_test_cab");
    write(
        &ir.join("manifest.yaml"),
        br#"manifest_version: 1
id: ir_audio_mode_test
display_name: IR Test
brand: testco
type: cab
backend: ir
parameters:
  - name: voicing
    values: [a, b]
captures:
  - values: { voicing: a }
    file: a.wav
  - values: { voicing: b }
    file: b.wav
"#,
    );
    write(&ir.join("a.wav"), b"fake");
    write(&ir.join("b.wav"), b"fake");

    plugin_loader::registry::init(&root);

    let nam_schema = project::block::schema_for_block_model("amp", "nam_audio_mode_test")
        .expect("NAM schema must resolve");
    assert_eq!(
        nam_schema.audio_mode,
        block_core::ModelAudioMode::MonoOnly,
        "NAM disk packages must declare MonoOnly — NAM DSP is mono-native and \
         the SDK can't safely host two simultaneous instances of the same model \
         (DualMono path created two and produced runtime feedback)"
    );

    let ir_schema = project::block::schema_for_block_model("cab", "ir_audio_mode_test")
        .expect("IR schema must resolve");
    assert_eq!(
        ir_schema.audio_mode,
        block_core::ModelAudioMode::MonoOnly,
        "IR disk packages should declare MonoOnly — single mono convolution \
         broadcast to stereo, no per-channel duplication"
    );
}
