//! Issue #938 — `openrig-render` must build VST3 blocks. Its bootstrap filled
//! the native and disk-package registries but never scanned VST3 bundles, so
//! every VST3 block failed with "VST3 plugin not found in catalog" even when
//! the GUI (which does scan) played it fine.
//!
//! Boots the catalogs exactly like the binary (`bootstrap::init_plugin_catalogs`)
//! and renders a chain holding one VST3 block. Real-plugin test, env-gated on
//! `OPENRIG_TEST_VST3_DIR` (the plugins `vst3/` dir, e.g.
//! `<OpenRig-plugins>/plugins/source/vst3`) like the #776 battery — the bundle
//! is 19 MB, too large to vendor. It skips loudly when the variable is unset.

use std::path::{Path, PathBuf};

use adapter_render::bootstrap::init_plugin_catalogs;
use adapter_render::cli::RenderArgs;
use adapter_render::render;

fn write_mono_source_wav(path: &Path) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for n in 0..24_000 {
        let s = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * n as f32 / 48_000.0).sin();
        writer.write_sample(s).unwrap();
        writer.write_sample(s).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn render_builds_a_vst3_block() {
    let Some(vst3_dir) = std::env::var_os("OPENRIG_TEST_VST3_DIR").map(PathBuf::from) else {
        eprintln!("[#938 VST3] OPENRIG_TEST_VST3_DIR not set — skipping");
        return;
    };
    let plugins_root = vst3_dir
        .parent()
        .expect("vst3 dir has a parent")
        .to_path_buf();
    init_plugin_catalogs(&[plugins_root], 48_000);

    let dir = std::env::temp_dir().join(format!("openrig-938-render-vst3-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let chain = dir.join("chain.yaml");
    std::fs::write(
        &chain,
        "id: vst3-938\nname: vst3\nblocks:\n- type: vst3\n  model: vst3_room_reverb\n  enabled: true\n  params: {}\n",
    )
    .unwrap();
    let input = dir.join("in.wav");
    let output = dir.join("out.wav");
    write_mono_source_wav(&input);

    let result = render(&RenderArgs {
        chain,
        input,
        output: output.clone(),
        start_s: None,
        end_s: None,
        duration_s: None,
        input_device: None,
        sample_rate_hz: 48_000,
        block_size: 512,
        bit_depth: 24,
        tail_ms: 500,
    });

    if let Err(e) = &result {
        panic!("a VST3 block must build in the renderer: {e}");
    }
    assert!(output.exists(), "the rendered WAV must be written");
}
