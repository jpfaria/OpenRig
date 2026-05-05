//! Extracts plugin metadata directly from each `crates/block-*/src/*.rs`
//! source file and emits a package under `plugins/source/`.
//!
//! No hardcoded plugin data. The tool reads the `.rs` files, parses out
//! constants and capture lists, and translates them into the new manifest
//! format. Backends:
//!
//! - `nam_*.rs` and `ir_*.rs` → grid-style packages whose parameters and
//!   captures come from the source file's `model_schema()` body and
//!   `CAPTURES` const.
//! - `lv2_*.rs` → bundle-style packages that pair the `PLUGIN_URI` from the
//!   source with the matching binaries actually present under
//!   `libs/lv2/<platform>/`.
//!
//! Run with `cargo run -p extract_plugins`.
//!
//! Issue: #287

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use plugin_loader::manifest::{
    Backend, BlockType, GridCapture, GridParameter, Lv2Slot, ParameterValue, PluginManifest,
};

const SOURCE_DIR: &str = "plugins/source";
const NAM_CAPTURES_ROOT: &str = "captures/nam";
const IR_CAPTURES_ROOT: &str = "captures/ir";
const LV2_DATA_ROOT: &str = "data/lv2";
const LV2_BIN_ROOT: &str = "libs/lv2";

fn main() -> Result<()> {
    let out = PathBuf::from(SOURCE_DIR);
    fs::create_dir_all(&out)?;

    let crates_root = Path::new("crates");
    let mut total = 0usize;
    let mut succeeded = 0usize;
    let mut failures: Vec<(PathBuf, String)> = Vec::new();

    for crate_entry in fs::read_dir(crates_root)? {
        let crate_entry = crate_entry?;
        let crate_path = crate_entry.path();
        let crate_name = match crate_entry.file_name().to_str() {
            Some(name) => name.to_string(),
            None => continue,
        };
        let Some(block_type) = block_type_for_crate(&crate_name) else {
            continue;
        };
        let src_dir = crate_path.join("src");
        if !src_dir.is_dir() {
            continue;
        }
        for source_entry in fs::read_dir(&src_dir)? {
            let source_entry = source_entry?;
            let source_path = source_entry.path();
            let Some(filename) = source_path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !is_plugin_source_file(filename) {
                continue;
            }
            total += 1;
            match extract_and_emit(&source_path, block_type, &out) {
                Ok(_id) => {
                    succeeded += 1;
                }
                Err(error) => {
                    failures.push((source_path.clone(), format!("{error:#}")));
                }
            }
        }
    }

    println!("\nProcessed: {total} source files");
    println!("Succeeded: {succeeded}");
    println!("Failed:    {}", failures.len());
    if !failures.is_empty() {
        println!("\nFailures:");
        for (path, error) in &failures {
            println!("  - {}: {error}", path.display());
        }
    }
    println!("\nNext: cargo run -p build_plugin_bundle");
    Ok(())
}

/// Map a crate directory name (e.g. `block-amp`) to the [`BlockType`] every
/// plugin in that crate belongs to. Returns `None` for crates that don't
/// host plugin sources (e.g. `block-core`, `block-routing`, infra crates).
fn block_type_for_crate(crate_name: &str) -> Option<BlockType> {
    Some(match crate_name {
        "block-amp" => BlockType::Amp,
        "block-preamp" => BlockType::Preamp,
        "block-cab" => BlockType::Cab,
        "block-body" => BlockType::Body,
        "block-gain" => BlockType::GainPedal,
        "block-mod" => BlockType::Mod,
        "block-delay" => BlockType::Delay,
        "block-reverb" => BlockType::Reverb,
        "block-filter" => BlockType::Filter,
        "block-dyn" => BlockType::Dyn,
        "block-pitch" => BlockType::Pitch,
        "block-wah" => BlockType::Wah,
        "block-util" => BlockType::Util,
        // block-ir is the generic IR loader; not migrated as a plugin.
        // block-nam is the NAM library wrapper; not a plugin.
        // block-core / block-routing / block-full-rig / feature-dsp / nam /
        // ir / vst3 / lv2 / infra-* / adapter-* / engine / domain /
        // application / project / ui-openrig — none of these host plugin
        // source files in the *_<id>.rs convention this tool reads.
        _ => return None,
    })
}

/// Plugin source files all start with one of the backend prefixes used by
/// the registry pattern: `nam_`, `ir_`, `lv2_`, or `native_`.
fn is_plugin_source_file(filename: &str) -> bool {
    if !filename.ends_with(".rs") {
        return false;
    }
    matches!(
        filename
            .split('_')
            .next()
            .map(|prefix| prefix.to_string()),
        Some(prefix) if matches!(prefix.as_str(), "nam" | "ir" | "lv2" | "native")
    )
}

fn extract_and_emit(source_file: &Path, block_type: BlockType, out: &Path) -> Result<String> {
    let source = fs::read_to_string(source_file)
        .with_context(|| format!("read {}", source_file.display()))?;

    let model_id = read_str_const(&source, "MODEL_ID", true)
        .ok_or_else(|| anyhow!("missing pub const MODEL_ID"))?;
    let display_name = read_str_const(&source, "DISPLAY_NAME", true)
        .ok_or_else(|| anyhow!("missing pub const DISPLAY_NAME"))?;
    let brand = read_str_const(&source, "BRAND", false);

    let filename = source_file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("source has no filename"))?;

    let manifest = if let Some(stem) = filename.strip_prefix("nam_") {
        let _ = stem;
        build_grid_manifest(&model_id, &display_name, brand.as_deref(), block_type, &source, "nam")?
    } else if let Some(stem) = filename.strip_prefix("ir_") {
        let _ = stem;
        build_grid_manifest(&model_id, &display_name, brand.as_deref(), block_type, &source, "ir")?
    } else if filename.starts_with("lv2_") {
        build_lv2_manifest(&model_id, &display_name, brand.as_deref(), block_type, &source)?
    } else {
        return Err(anyhow!("filename `{filename}` is neither nam_/ir_/lv2_"));
    };

    write_package(out, &manifest, source_file, &source)?;
    Ok(model_id)
}

// ─── source-file scanners ────────────────────────────────────────────────────

/// Find the value of a `[pub] const NAME: &str = "...";` line.
fn read_str_const(source: &str, name: &str, must_be_pub: bool) -> Option<String> {
    let needle = if must_be_pub {
        format!("pub const {name}: &str = ")
    } else {
        format!("const {name}: &str = ")
    };
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&needle) {
            let rest = &trimmed[needle.len()..];
            return read_string_literal(rest);
        }
        // also allow non-pub when caller permitted pub-or-not via must_be_pub=false
        if !must_be_pub {
            let alt = format!("pub const {name}: &str = ");
            if trimmed.starts_with(&alt) {
                let rest = &trimmed[alt.len()..];
                return read_string_literal(rest);
            }
        }
    }
    None
}

/// Reads the first `"..."` literal out of a slice; backslash escapes are
/// preserved as-is (we don't decode them — we only need the string content
/// to copy into a YAML field).
fn read_string_literal(input: &str) -> Option<String> {
    let mut chars = input.chars();
    if chars.next() != Some('"') {
        return None;
    }
    let mut value = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            value.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            return Some(value);
        }
        value.push(ch);
    }
    None
}

/// Read the slice of the `const CAPTURES` array literal — the body between
/// the `&[` that opens the array and its matching `]`.
///
/// The signature contains its own brackets (`&[CaptureType]`), so we skip
/// past the `=` first and only then look for the array's opening `[`.
fn read_captures_block(source: &str) -> Option<&str> {
    let needle = "const CAPTURES";
    let start = source.find(needle)?;
    let after = &source[start..];
    let eq_offset = after.find('=')?;
    let from_eq = &after[eq_offset..];
    let array_start = from_eq.find('[')?;
    let body = &from_eq[array_start + 1..];
    let mut depth = 1usize;
    for (offset, ch) in body.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&body[..offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Inside a CAPTURES body, walk top-level entries. Each entry is either a
/// `(...)` tuple or a `capture!(...)` macro call. Returns the inner argument
/// list (between `(` and matching `)`) for each entry. Comments are stripped
/// per line before scanning.
fn read_capture_entries(body: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    let bytes = body.as_bytes();
    while cursor < bytes.len() {
        // Find next `(` skipping whitespace and `,`.
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if byte == b'(' {
                break;
            }
            cursor += 1;
        }
        if cursor >= bytes.len() {
            break;
        }
        let inner_start = cursor + 1;
        let mut depth = 1usize;
        let mut scan = inner_start;
        while scan < bytes.len() && depth > 0 {
            match bytes[scan] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            scan += 1;
        }
        if depth != 0 {
            break;
        }
        let inner = &body[inner_start..scan];
        entries.push(inner);
        cursor = scan + 1;
    }
    entries
}

/// Pull every double-quoted literal out of an arg list.
fn read_string_literals_in(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let bytes = args.as_bytes();
    while cursor < bytes.len() {
        if bytes[cursor] != b'"' {
            cursor += 1;
            continue;
        }
        let start = cursor + 1;
        let mut scan = start;
        let mut escaped = false;
        while scan < bytes.len() {
            let byte = bytes[scan];
            if escaped {
                escaped = false;
                scan += 1;
                continue;
            }
            if byte == b'\\' {
                escaped = true;
                scan += 1;
                continue;
            }
            if byte == b'"' {
                break;
            }
            scan += 1;
        }
        if scan >= bytes.len() {
            break;
        }
        out.push(args[start..scan].to_string());
        cursor = scan + 1;
    }
    out
}

/// Walk the source for `enum_parameter("name", "Display", ..., &[(v,l), ...])`
/// invocations and return one [`GridParameter`] per call, in source order.
fn read_enum_parameters(source: &str) -> Vec<GridParameter> {
    let mut parameters = Vec::new();
    let mut cursor = 0usize;
    let needle = "enum_parameter(";
    while let Some(found) = source[cursor..].find(needle) {
        let arg_start = cursor + found + needle.len();
        let mut depth = 1usize;
        let mut scan = arg_start;
        let bytes = source.as_bytes();
        while scan < bytes.len() && depth > 0 {
            match bytes[scan] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            scan += 1;
        }
        if depth != 0 {
            break;
        }
        let args = &source[arg_start..scan];
        // `enum_parameter(name, display, group, default, &[(v, l), ...])`
        // produces literals in order:
        //   [0] name
        //   [1] display_name
        //   [2] group string (inside Some("..."))   ← skipped
        //   [3] default value (inside Some("..."))  ← skipped
        //   [4..] alternating value, label, value, label, ...
        // Some calls pass `None` for group or default — when that happens
        // those literal slots collapse, so we anchor on the values slice
        // by finding the start of the `&[` block instead of indexing.
        let literals = read_string_literals_in(args);
        let values_slice_start = match args.find("&[") {
            Some(offset) => offset,
            None => {
                cursor = scan + 1;
                continue;
            }
        };
        let values_segment = &args[values_slice_start..];
        let values_literals = read_string_literals_in(values_segment);
        if literals.len() >= 2 && !values_literals.is_empty() {
            let name = literals[0].clone();
            let display = literals[1].clone();
            // Inside `&[(v, l), (v, l), ...]` literals alternate v, l.
            let values: Vec<ParameterValue> = values_literals
                .iter()
                .step_by(2)
                .map(|value| ParameterValue::Text(value.clone()))
                .collect();
            parameters.push(GridParameter {
                name,
                display_name: Some(display),
                values,
            });
        }
        cursor = scan + 1;
    }
    parameters
}

// ─── manifest builders ───────────────────────────────────────────────────────

fn build_grid_manifest(
    model_id: &str,
    display_name: &str,
    brand: Option<&str>,
    block_type: BlockType,
    source: &str,
    flavor: &str,
) -> Result<PluginManifest> {
    let parameters = read_enum_parameters(source);
    let captures_body = read_captures_block(source)
        .ok_or_else(|| anyhow!("no `const CAPTURES` block found in source"))?;
    let entries = read_capture_entries(captures_body);

    // For NAM/IR captures, the file path is the LAST string literal in each
    // entry; the parameter values precede it. We use the parameter list to
    // know how many values to bind by position.
    let mut captures: Vec<GridCapture> = Vec::new();
    for entry in entries {
        let literals = read_string_literals_in(entry);
        if literals.is_empty() {
            continue;
        }
        let file_relative = literals.last().unwrap().clone();
        let value_strs = &literals[..literals.len() - 1];
        let mut values = BTreeMap::new();
        for (parameter, value) in parameters.iter().zip(value_strs.iter()) {
            values.insert(
                parameter.name.clone(),
                ParameterValue::Text(value.clone()),
            );
        }
        captures.push(GridCapture {
            values,
            file: PathBuf::from(strip_path_prefix(&file_relative, flavor)),
        });
    }

    let backend = match flavor {
        "nam" => Backend::Nam {
            parameters,
            captures,
        },
        "ir" => Backend::Ir {
            parameters,
            captures,
        },
        other => return Err(anyhow!("unknown grid flavor `{other}`")),
    };

    Ok(PluginManifest {
        manifest_version: 1,
        id: model_id.to_string(),
        display_name: display_name.to_string(),
        author: None,
        description: None,
        inspired_by: brand.map(str::to_string),
        block_type,
        backend,
    })
}

/// Source paths look like `cabs/<model>/file.wav` or
/// `full_rigs/<model>/file.nam`. Inside the new package we put the assets
/// under `ir/` or `captures/`. Strip the leading classification segment so
/// the in-package paths line up with the package layout we copy into.
fn strip_path_prefix(raw: &str, flavor: &str) -> String {
    // Accept everything after the model directory. If the raw path starts
    // with something like `cabs/<model>/` or `full_rigs/<model>/`, drop the
    // first segment. Otherwise return as-is.
    let mut segments = raw.split('/');
    let _classification = segments.next();
    let _model_dir = segments.next();
    let rest: Vec<&str> = segments.collect();
    if rest.is_empty() {
        return raw.to_string();
    }
    let basename = rest.join("/");
    let prefix = match flavor {
        "ir" => "ir",
        "nam" => "captures",
        _ => "",
    };
    format!("{prefix}/{basename}")
}

fn build_lv2_manifest(
    model_id: &str,
    display_name: &str,
    brand: Option<&str>,
    block_type: BlockType,
    source: &str,
) -> Result<PluginManifest> {
    let plugin_uri = read_str_const(source, "PLUGIN_URI", false)
        .ok_or_else(|| anyhow!("missing const PLUGIN_URI"))?;
    let plugin_dir = read_str_const(source, "PLUGIN_DIR", false)
        .ok_or_else(|| anyhow!("missing const PLUGIN_DIR"))?;
    // PLUGIN_BINARY is gated by cfg per platform; pick the bare filename.
    let binary_filename = read_lv2_binary_filename(source)
        .ok_or_else(|| anyhow!("could not extract LV2 binary filename"))?;

    let bundle_path = PathBuf::from(format!("bundles/{model_id}.lv2"));
    let mut binaries = BTreeMap::new();
    let host_to_slot: &[(&str, Lv2Slot)] = &[
        ("macos-universal", Lv2Slot::MacosUniversal),
        ("linux-x86_64", Lv2Slot::LinuxX86_64),
        ("linux-aarch64", Lv2Slot::LinuxAarch64),
        ("windows-x64", Lv2Slot::WindowsX86_64),
        ("windows-arm64", Lv2Slot::WindowsAarch64),
    ];
    for (host_dir, slot) in host_to_slot {
        let candidate = PathBuf::from(LV2_BIN_ROOT)
            .join(host_dir)
            .join(filename_for_platform(&binary_filename, host_dir));
        if candidate.is_file() {
            let slot_name = slot_directory_name(slot);
            let in_pkg = bundle_path
                .join(slot_name)
                .join(filename_for_platform(&binary_filename, host_dir));
            binaries.insert(*slot, in_pkg);
        }
    }

    if binaries.is_empty() {
        return Err(anyhow!("no LV2 binaries found under {LV2_BIN_ROOT}/* matching {binary_filename}"));
    }

    let _ = plugin_dir;

    Ok(PluginManifest {
        manifest_version: 1,
        id: model_id.to_string(),
        display_name: display_name.to_string(),
        author: None,
        description: None,
        inspired_by: brand.map(str::to_string),
        block_type,
        backend: Backend::Lv2 {
            plugin_uri,
            bundle_path,
            binaries,
        },
    })
}

/// `PLUGIN_BINARY` is split across `#[cfg(target_os = ...)]` branches in
/// the source, but the *base* filename (without OS-specific extension)
/// matches the disk layout under `libs/lv2/`. Find any of the per-OS
/// declarations and strip its OS-specific extension to recover the base.
fn read_lv2_binary_filename(source: &str) -> Option<String> {
    for needle in ["PLUGIN_BINARY"] {
        if let Some(value) = read_str_const(source, needle, false) {
            return Some(value);
        }
    }
    // Fallback: scan for any literal ending in .so / .dll / .dylib
    for line in source.lines() {
        for ext in [".dylib", ".so", ".dll"] {
            if line.contains(ext) && line.contains('"') {
                if let Some(value) = read_string_literal(line.trim_start_matches(|c: char| c != '"')) {
                    if value.ends_with(ext) {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

/// Map a base filename (e.g. `PhaserII.dylib`) to the version expected on
/// disk for the given platform directory under `libs/lv2/`.
fn filename_for_platform(base: &str, host_dir: &str) -> String {
    let stem = base
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(base);
    match host_dir {
        "macos-universal" => format!("{stem}.dylib"),
        "windows-x64" | "windows-arm64" => format!("{stem}.dll"),
        "linux-x86_64" | "linux-aarch64" => format!("{stem}.so"),
        _ => base.to_string(),
    }
}

fn slot_directory_name(slot: &Lv2Slot) -> String {
    serde_yaml::to_value(slot)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| format!("{slot:?}"))
}

// ─── package writer (also copies the actual asset files) ─────────────────────

fn write_package(
    out: &Path,
    manifest: &PluginManifest,
    source_file: &Path,
    source_text: &str,
) -> Result<()> {
    let package_dir = out.join(&manifest.id);
    if package_dir.exists() {
        fs::remove_dir_all(&package_dir)?;
    }
    fs::create_dir_all(&package_dir)?;

    match &manifest.backend {
        Backend::Nam { captures, .. } => {
            for capture in captures {
                copy_asset(
                    &resolve_nam_capture_source(source_file, source_text, &capture.file)?,
                    &package_dir.join(&capture.file),
                )?;
            }
        }
        Backend::Ir { captures, .. } => {
            for capture in captures {
                copy_asset(
                    &resolve_ir_capture_source(source_file, source_text, &capture.file)?,
                    &package_dir.join(&capture.file),
                )?;
            }
        }
        Backend::Lv2 {
            bundle_path,
            binaries,
            ..
        } => {
            // Copy TTLs from data/lv2/<dir>/ into bundle_path
            let plugin_dir = read_str_const(source_text, "PLUGIN_DIR", false)
                .ok_or_else(|| anyhow!("PLUGIN_DIR missing for LV2 copy"))?;
            let ttl_source = PathBuf::from(LV2_DATA_ROOT).join(&plugin_dir);
            let bundle_dest = package_dir.join(bundle_path);
            fs::create_dir_all(&bundle_dest)?;
            if ttl_source.is_dir() {
                for entry in fs::read_dir(&ttl_source)? {
                    let entry = entry?;
                    if entry.file_type()?.is_file() {
                        fs::copy(entry.path(), bundle_dest.join(entry.file_name()))?;
                    }
                }
            }
            // Copy per-slot binaries
            for (slot, in_pkg) in binaries {
                let dst = package_dir.join(in_pkg);
                fs::create_dir_all(dst.parent().unwrap())?;
                let host_dir = host_dir_for_slot(slot);
                let filename = in_pkg
                    .file_name()
                    .ok_or_else(|| anyhow!("binary path has no filename"))?;
                let src = PathBuf::from(LV2_BIN_ROOT).join(host_dir).join(filename);
                fs::copy(&src, &dst).with_context(|| format!("copy {}", src.display()))?;
            }
        }
    }

    let yaml = serde_yaml::to_string(manifest)?;
    fs::write(package_dir.join("manifest.yaml"), yaml)?;
    Ok(())
}

fn host_dir_for_slot(slot: &Lv2Slot) -> &'static str {
    match slot {
        Lv2Slot::MacosUniversal => "macos-universal",
        Lv2Slot::WindowsX86_64 => "windows-x64",
        Lv2Slot::WindowsAarch64 => "windows-arm64",
        Lv2Slot::LinuxX86_64 => "linux-x86_64",
        Lv2Slot::LinuxAarch64 => "linux-aarch64",
    }
}

fn resolve_nam_capture_source(
    _source_file: &Path,
    source_text: &str,
    in_pkg_path: &Path,
) -> Result<PathBuf> {
    // captures[N].file in the manifest is `captures/<basename.nam>`. Look up
    // the full source path in the original CAPTURES block by basename match.
    let basename = in_pkg_path
        .file_name()
        .ok_or_else(|| anyhow!("no basename for {}", in_pkg_path.display()))?
        .to_string_lossy()
        .to_string();
    let captures_body = read_captures_block(source_text)
        .ok_or_else(|| anyhow!("no captures block"))?;
    for entry in read_capture_entries(captures_body) {
        let literals = read_string_literals_in(entry);
        if let Some(path_str) = literals.last() {
            if Path::new(path_str)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                == Some(basename.clone())
            {
                return Ok(PathBuf::from(NAM_CAPTURES_ROOT).join(path_str));
            }
        }
    }
    Err(anyhow!("could not resolve NAM source for {basename}"))
}

fn resolve_ir_capture_source(
    _source_file: &Path,
    source_text: &str,
    in_pkg_path: &Path,
) -> Result<PathBuf> {
    let basename = in_pkg_path
        .file_name()
        .ok_or_else(|| anyhow!("no basename"))?
        .to_string_lossy()
        .to_string();
    let captures_body = read_captures_block(source_text)
        .ok_or_else(|| anyhow!("no captures block"))?;
    for entry in read_capture_entries(captures_body) {
        let literals = read_string_literals_in(entry);
        if let Some(path_str) = literals.last() {
            if let Some(name) = Path::new(path_str).file_name() {
                if name.to_string_lossy() == basename {
                    let candidate = PathBuf::from(IR_CAPTURES_ROOT).join(path_str);
                    if candidate.is_file() {
                        return Ok(candidate);
                    }
                    // Fallback: source has stale `_3` suffix that the real
                    // file lacks. Try stripping it and re-resolving.
                    let stripped = path_str.replace("_3.wav", ".wav");
                    let candidate_stripped = PathBuf::from(IR_CAPTURES_ROOT).join(&stripped);
                    if candidate_stripped.is_file() {
                        return Ok(candidate_stripped);
                    }
                }
            }
        }
    }
    Err(anyhow!("could not resolve IR source for {basename}"))
}

fn copy_asset(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    Ok(())
}
