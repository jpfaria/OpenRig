//! Hand-written proof-of-concept migration: 1 NAM + 1 IR + 1 LV2.
//!
//! This is **not** the real migration tool. It picks three representative
//! existing plugins, hardcodes their structure, and emits packages under
//! `plugins/source/`. Goal: confirm the format, layout, and asset copying
//! logic before scaling to all 561 MODEL_DEFINITION instances.
//!
//! Usage:
//!
//! ```text
//! cargo run -p migrate_proof
//! cargo run -p build_plugin_bundle
//! ```
//!
//! Issue: #287

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use plugin_loader::manifest::{
    Backend, BlockType, GridCapture, GridParameter, Lv2Slot, ParameterValue, PluginManifest,
};

const SOURCE_DIR: &str = "plugins/source";
const NAM_CAPTURES_ROOT: &str = "captures/nam";
const IR_CAPTURES_ROOT: &str = "captures/ir";
const LV2_DATA_ROOT: &str = "data/lv2";
const LV2_BIN_ROOT: &str = "libs/lv2";

fn main() -> Result<()> {
    let source_dir = PathBuf::from(SOURCE_DIR);
    fs::create_dir_all(&source_dir).context("create plugins/source")?;

    emit_nam_ampeg_svt_classic(&source_dir)?;
    emit_ir_ampeg_svt_8x10(&source_dir)?;
    emit_lv2_caps_phaser2(&source_dir)?;

    println!("\nDone. Run `cargo run -p build_plugin_bundle` to validate + bundle.");
    Ok(())
}

// ─── NAM ─────────────────────────────────────────────────────────────────────

fn emit_nam_ampeg_svt_classic(source_dir: &Path) -> Result<()> {
    let id = "ampeg_svt_classic";
    let display_name = "SVT Classic";

    let captures = [
        ("standard", "md421", "ampeg_svt_md421.nam"),
        ("standard", "sm57", "ampeg_svt_sm57.nam"),
        ("ultra_hi", "md421", "ampeg_svt_ultra_hi_md421.nam"),
        ("ultra_hi", "sm57", "ampeg_svt_ultra_hi_sm57.nam"),
        ("ultra_lo", "md421", "ampeg_svt_ultra_lo_md421.nam"),
        ("ultra_lo", "sm57", "ampeg_svt_ultra_lo_sm57.nam"),
    ];

    let package_dir = source_dir.join(id);
    let captures_dir_in_pkg = "captures";

    let manifest = PluginManifest {
        manifest_version: 1,
        id: id.to_string(),
        display_name: display_name.to_string(),
        author: None,
        description: Some("Ampeg SVT Classic captured at full_rigs grid (tone × mic).".to_string()),
        inspired_by: Some("Ampeg SVT".to_string()),
        block_type: BlockType::Amp,
        backend: Backend::Nam {
            parameters: vec![
                GridParameter {
                    name: "tone".to_string(),
                    display_name: Some("Tone".to_string()),
                    values: vec![
                        ParameterValue::Text("standard".to_string()),
                        ParameterValue::Text("ultra_hi".to_string()),
                        ParameterValue::Text("ultra_lo".to_string()),
                    ],
                },
                GridParameter {
                    name: "mic".to_string(),
                    display_name: Some("Mic".to_string()),
                    values: vec![
                        ParameterValue::Text("md421".to_string()),
                        ParameterValue::Text("sm57".to_string()),
                    ],
                },
            ],
            captures: captures
                .iter()
                .map(|(tone, mic, file)| GridCapture {
                    values: BTreeMap::from([
                        ("tone".to_string(), ParameterValue::Text((*tone).to_string())),
                        ("mic".to_string(), ParameterValue::Text((*mic).to_string())),
                    ]),
                    file: PathBuf::from(captures_dir_in_pkg).join(file),
                })
                .collect(),
        },
    };

    write_package(&package_dir, &manifest, |pkg| {
        let dest_dir = pkg.join(captures_dir_in_pkg);
        fs::create_dir_all(&dest_dir)?;
        for (_, _, file) in captures.iter() {
            let src = PathBuf::from(NAM_CAPTURES_ROOT)
                .join("full_rigs")
                .join(id)
                .join(file);
            let dst = dest_dir.join(file);
            fs::copy(&src, &dst).with_context(|| format!("copy {}", src.display()))?;
        }
        Ok(())
    })
}

// ─── IR ──────────────────────────────────────────────────────────────────────

fn emit_ir_ampeg_svt_8x10(source_dir: &Path) -> Result<()> {
    let id = "ampeg_svt_8x10";
    let display_name = "SVT 4x10/8x10";

    let captures = [
        ("d6", "ah", "ampeg_8x10_d6_ah.wav"),
        ("57", "ah", "ampeg_8x10_57_ah.wav"),
        ("4033", "ah", "ampeg_8x10_4033_ah.wav"),
        ("4033", "a107", "ampeg_8x10_4033_a107.wav"),
        ("e602", "a107", "ampeg_8x10_e602_a107.wav"),
        ("beta52", "svt_di", "ampeg_svt_beta52.wav"),
        ("neumann", "svt_di", "ampeg_svt_bright_neumann.wav"),
        ("di_out", "svt_di", "ampeg_svt_d_i_out.wav"),
    ];

    let package_dir = source_dir.join(id);
    let ir_dir_in_pkg = "ir";

    let mic_values = vec![
        "d6", "57", "4033", "e602", "beta52", "neumann", "di_out",
    ];
    let position_values = vec!["ah", "a107", "svt_di"];

    let manifest = PluginManifest {
        manifest_version: 1,
        id: id.to_string(),
        display_name: display_name.to_string(),
        author: None,
        description: Some("Ampeg 8x10 cab IR with sparse mic × position grid.".to_string()),
        inspired_by: Some("Ampeg SVT 8x10".to_string()),
        block_type: BlockType::Cab,
        backend: Backend::Ir {
            parameters: vec![
                GridParameter {
                    name: "mic".to_string(),
                    display_name: Some("Mic".to_string()),
                    values: mic_values
                        .iter()
                        .map(|v| ParameterValue::Text((*v).to_string()))
                        .collect(),
                },
                GridParameter {
                    name: "position".to_string(),
                    display_name: Some("Position".to_string()),
                    values: position_values
                        .iter()
                        .map(|v| ParameterValue::Text((*v).to_string()))
                        .collect(),
                },
            ],
            captures: captures
                .iter()
                .map(|(mic, position, file)| GridCapture {
                    values: BTreeMap::from([
                        ("mic".to_string(), ParameterValue::Text((*mic).to_string())),
                        (
                            "position".to_string(),
                            ParameterValue::Text((*position).to_string()),
                        ),
                    ]),
                    file: PathBuf::from(ir_dir_in_pkg).join(file),
                })
                .collect(),
        },
    };

    write_package(&package_dir, &manifest, |pkg| {
        let dest_dir = pkg.join(ir_dir_in_pkg);
        fs::create_dir_all(&dest_dir)?;
        for (_, _, file) in captures.iter() {
            let src = PathBuf::from(IR_CAPTURES_ROOT).join("cabs").join(id).join(file);
            let dst = dest_dir.join(file);
            fs::copy(&src, &dst).with_context(|| format!("copy {}", src.display()))?;
        }
        Ok(())
    })
}

// ─── LV2 ─────────────────────────────────────────────────────────────────────

fn emit_lv2_caps_phaser2(source_dir: &Path) -> Result<()> {
    let id = "lv2_caps_phaser2";
    let display_name = "CAPS Phaser II";
    let plugin_uri = "http://moddevices.com/plugins/caps/PhaserII";

    // (manifest slot, source platform dir under libs/lv2/, source binary filename)
    let binary_slots = [
        (Lv2Slot::MacosUniversal, "macos-universal", "PhaserII.dylib"),
        (Lv2Slot::LinuxX86_64, "linux-x86_64", "PhaserII.so"),
        (Lv2Slot::LinuxAarch64, "linux-aarch64", "PhaserII.so"),
    ];

    let bundle_dir_in_pkg = format!("bundles/{id}.lv2");
    let package_dir = source_dir.join(id);
    let ttl_source_dir = PathBuf::from(LV2_DATA_ROOT).join("mod-caps-PhaserII");

    // Build the slot map for the manifest, pointing at where each binary
    // will land inside the package after the copy below.
    let mut binaries = BTreeMap::new();
    for (slot, _src_plat, src_filename) in binary_slots.iter() {
        // Slot name in the package layout uses the manifest's canonical name,
        // which we get by serializing the slot through serde_yaml. Cheap and
        // keeps the wire format and on-disk layout in agreement.
        let slot_name = serde_yaml::to_value(slot)
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{slot:?}"));
        let in_pkg = PathBuf::from(&bundle_dir_in_pkg)
            .join(&slot_name)
            .join(src_filename);
        binaries.insert(*slot, in_pkg);
    }

    let manifest = PluginManifest {
        manifest_version: 1,
        id: id.to_string(),
        display_name: display_name.to_string(),
        author: None,
        description: Some("MOD-CAPS PhaserII LV2 plugin packaged for OpenRig.".to_string()),
        inspired_by: None,
        block_type: BlockType::Mod,
        backend: Backend::Lv2 {
            plugin_uri: plugin_uri.to_string(),
            bundle_path: PathBuf::from(&bundle_dir_in_pkg),
            binaries,
        },
    };

    write_package(&package_dir, &manifest, |pkg| {
        let bundle_dir = pkg.join(&bundle_dir_in_pkg);
        fs::create_dir_all(&bundle_dir)?;

        // 1. Copy every .ttl from the LV2 data dir into the bundle root.
        for entry in fs::read_dir(&ttl_source_dir)
            .with_context(|| format!("read {}", ttl_source_dir.display()))?
        {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let dst = bundle_dir.join(entry.file_name());
                fs::copy(entry.path(), &dst)?;
            }
        }

        // 2. Copy each platform binary into bundle/<slot>/<filename>.
        for (slot, src_plat, src_filename) in binary_slots.iter() {
            let slot_name = serde_yaml::to_value(slot)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| format!("{slot:?}"));
            let slot_dir = bundle_dir.join(&slot_name);
            fs::create_dir_all(&slot_dir)?;
            let src = PathBuf::from(LV2_BIN_ROOT).join(src_plat).join(src_filename);
            let dst = slot_dir.join(src_filename);
            fs::copy(&src, &dst).with_context(|| format!("copy {}", src.display()))?;
        }

        Ok(())
    })
}

// ─── shared helpers ──────────────────────────────────────────────────────────

fn write_package<F>(package_dir: &Path, manifest: &PluginManifest, copy_assets: F) -> Result<()>
where
    F: FnOnce(&Path) -> Result<()>,
{
    if package_dir.exists() {
        fs::remove_dir_all(package_dir)
            .with_context(|| format!("clean {}", package_dir.display()))?;
    }
    fs::create_dir_all(package_dir)?;
    copy_assets(package_dir)?;

    let yaml = serde_yaml::to_string(manifest).context("serialize manifest")?;
    let manifest_path = package_dir.join("manifest.yaml");
    fs::write(&manifest_path, yaml).with_context(|| format!("write {}", manifest_path.display()))?;

    println!(
        "  ok    {} ({} -> {})",
        manifest.id,
        match &manifest.backend {
            Backend::Nam { .. } => "nam",
            Backend::Ir { .. } => "ir",
            Backend::Lv2 { .. } => "lv2",
        },
        package_dir.display()
    );
    Ok(())
}
