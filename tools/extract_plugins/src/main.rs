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
const THUMBNAILS_ROOT: &str = "assets/blocks/thumbnails";
const PHOTOS_ROOT: &str = "assets/models/photos";
const SCREENSHOTS_ROOT: &str = "assets/blocks/screenshots";
const BRANDS_ROOT: &str = "assets/brands";
const METADATA_FILE: &str = "assets/blocks/metadata/en-US.yaml";

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

/// Plugin source files start with one of three backend prefixes the tool
/// can migrate: `nam_`, `ir_`, or `lv2_`. `native_*.rs` files exist in the
/// repo too but they're DSP-in-engine, not external packages — silently
/// skipped so they don't show up in the failure report.
fn is_plugin_source_file(filename: &str) -> bool {
    if !filename.ends_with(".rs") {
        return false;
    }
    let stem_prefix = filename.split('_').next().unwrap_or("");
    matches!(stem_prefix, "nam" | "ir" | "lv2")
}

fn extract_and_emit(source_file: &Path, block_type: BlockType, out: &Path) -> Result<String> {
    let source = fs::read_to_string(source_file)
        .with_context(|| format!("read {}", source_file.display()))?;

    let raw_model_id = read_str_const(&source, "MODEL_ID", true)
        .ok_or_else(|| anyhow!("missing pub const MODEL_ID"))?;
    let display_name = read_str_const(&source, "DISPLAY_NAME", true)
        .ok_or_else(|| anyhow!("missing pub const DISPLAY_NAME"))?;
    let brand = read_str_const(&source, "BRAND", false);

    let filename = source_file
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("source has no filename"))?;

    // Backend lives in the filename prefix; we use it to (a) decide the
    // package folder, (b) ensure the manifest id is prefixed.
    let backend_prefix = if filename.starts_with("nam_") {
        "nam"
    } else if filename.starts_with("ir_") {
        "ir"
    } else if filename.starts_with("lv2_") {
        "lv2"
    } else {
        return Err(anyhow!("filename `{filename}` is neither nam_/ir_/lv2_"));
    };

    // Folder name = id with backend prefix stripped.
    let folder_id = raw_model_id
        .strip_prefix(&format!("{backend_prefix}_"))
        .unwrap_or(&raw_model_id)
        .to_string();
    // Manifest id = always prefixed with backend.
    let model_id = if raw_model_id.starts_with(&format!("{backend_prefix}_")) {
        raw_model_id.clone()
    } else {
        format!("{backend_prefix}_{raw_model_id}")
    };

    let mut manifest = match backend_prefix {
        "nam" => build_grid_manifest(&model_id, &display_name, brand.as_deref(), block_type, &source, "nam")?,
        "ir" => build_grid_manifest(&model_id, &display_name, brand.as_deref(), block_type, &source, "ir")?,
        "lv2" => build_lv2_manifest(&model_id, &display_name, brand.as_deref(), block_type, &source)?,
        _ => unreachable!("backend_prefix already validated above"),
    };

    // Promote brand from `inspired_by` (where the grid/lv2 builders parked
    // it) into the dedicated `brand` field, where it semantically belongs.
    manifest.brand = manifest.inspired_by.clone();
    manifest.inspired_by = None;

    // Probe the asset directories for matching files. Best-effort: missing
    // assets just leave the corresponding manifest field as None.
    if locate_thumbnail(&model_id, block_type).is_some() {
        manifest.thumbnail = Some(PathBuf::from("assets/thumbnail.png"));
    }
    if locate_photo(&model_id).is_some() {
        manifest.photo = Some(PathBuf::from("assets/photo.png"));
    }
    if locate_screenshot(&model_id, block_type).is_some() {
        manifest.screenshot = Some(PathBuf::from("assets/screenshot.png"));
    }
    if let Some(brand_value) = &manifest.brand {
        if let Some((src, ext)) = locate_brand_logo(brand_value) {
            let _ = src;
            manifest.brand_logo = Some(PathBuf::from(format!("assets/brand_logo.{ext}")));
        }
    }
    if let Some(metadata) = lookup_metadata(&model_id) {
        if manifest.description.is_none() {
            manifest.description = metadata.description.clone();
        }
        manifest.license = metadata.license.clone();
        manifest.homepage = metadata.homepage.clone();
    }

    let backend_root = out.join(backend_prefix);
    fs::create_dir_all(&backend_root)?;
    write_package(&backend_root, &folder_id, &manifest, source_file, &source)?;
    Ok(format!("{backend_prefix}/{folder_id}"))
}

fn locate_thumbnail(model_id: &str, block_type: BlockType) -> Option<PathBuf> {
    let dir = match block_type {
        BlockType::GainPedal => "gain",
        BlockType::Preamp => "preamp",
        BlockType::Amp => "amp",
        BlockType::Cab => "cab",
        BlockType::Body => "body",
        BlockType::Reverb => "reverb",
        BlockType::Delay => "delay",
        BlockType::Mod => "modulation",
        BlockType::Filter => "filter",
        BlockType::Dyn => "dynamics",
        BlockType::Wah => "wah",
        BlockType::Pitch => "pitch",
        BlockType::Util => "util",
    };
    let candidate = PathBuf::from(THUMBNAILS_ROOT)
        .join(dir)
        .join(format!("{model_id}.png"));
    candidate.is_file().then_some(candidate)
}

fn locate_photo(model_id: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(PHOTOS_ROOT).join(format!("{model_id}.png"));
    candidate.is_file().then_some(candidate)
}

fn locate_screenshot(model_id: &str, block_type: BlockType) -> Option<PathBuf> {
    let dir = match block_type {
        BlockType::GainPedal => "gain",
        BlockType::Preamp => "preamp",
        BlockType::Amp => "amp",
        BlockType::Cab => "cab",
        BlockType::Body => "body",
        BlockType::Reverb => "reverb",
        BlockType::Delay => "delay",
        BlockType::Mod => "modulation",
        BlockType::Filter => "filter",
        BlockType::Dyn => "dynamics",
        BlockType::Wah => "wah",
        BlockType::Pitch => "pitch",
        BlockType::Util => "utility",
    };
    let candidate = PathBuf::from(SCREENSHOTS_ROOT)
        .join(dir)
        .join(format!("{model_id}.png"));
    candidate.is_file().then_some(candidate)
}

/// Brand logos can be `.svg` or `.png`. Returns the path plus its extension
/// so the package writer can preserve the original format.
fn locate_brand_logo(brand: &str) -> Option<(PathBuf, &'static str)> {
    for ext in ["svg", "png"] {
        let candidate = PathBuf::from(BRANDS_ROOT)
            .join(brand)
            .join(format!("logo.{ext}"));
        if candidate.is_file() {
            return Some((candidate, ext));
        }
    }
    None
}

/// Lazily-parsed metadata index. Keys by plugin id, holds whatever fields
/// the YAML carries for that id.
#[derive(Default, Clone)]
struct PluginMetadata {
    description: Option<String>,
    license: Option<String>,
    homepage: Option<String>,
}

fn lookup_metadata(model_id: &str) -> Option<PluginMetadata> {
    use std::sync::OnceLock;
    static INDEX: OnceLock<BTreeMap<String, PluginMetadata>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let bytes = match fs::read(METADATA_FILE) {
            Ok(bytes) => bytes,
            Err(_) => return BTreeMap::new(),
        };
        let value: serde_yaml::Value = match serde_yaml::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return BTreeMap::new(),
        };
        let plugins = value.get("plugins").and_then(|node| node.as_mapping());
        let mut out = BTreeMap::new();
        if let Some(plugins) = plugins {
            for (key, entry) in plugins {
                let Some(id) = key.as_str() else { continue };
                let mapping = match entry.as_mapping() {
                    Some(mapping) => mapping,
                    None => continue,
                };
                let read_str = |field: &str| -> Option<String> {
                    mapping
                        .get(serde_yaml::Value::String(field.to_string()))
                        .and_then(|node| node.as_str())
                        .map(str::to_string)
                };
                out.insert(
                    id.to_string(),
                    PluginMetadata {
                        description: read_str("description"),
                        license: read_str("license"),
                        homepage: read_str("homepage"),
                    },
                );
            }
        }
        out
    });
    index.get(model_id).cloned()
}

// ─── source-file scanners ────────────────────────────────────────────────────

/// Find the value of a `[pub] const NAME: &str = "...";` declaration.
///
/// Handles both single-line forms and the multi-line variant common in
/// GxPlugins where the value is wrapped onto the next line:
///
/// ```ignore
/// const PLUGIN_URI: &str =
///     "http://...";
/// ```
fn read_str_const(source: &str, name: &str, must_be_pub: bool) -> Option<String> {
    let candidates: &[String] = if must_be_pub {
        &[format!("pub const {name}: &str =")]
    } else {
        &[
            format!("const {name}: &str ="),
            format!("pub const {name}: &str ="),
        ]
    };

    for candidate in candidates {
        if let Some(start) = source.find(candidate.as_str()) {
            // Skip past the candidate prefix; then scan forward for the
            // first `"` (the start of the literal). Whitespace/newlines
            // between `=` and `"` are accepted.
            let after = &source[start + candidate.len()..];
            let quote_offset = after.find('"')?;
            return read_string_literal(&after[quote_offset..]);
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

/// Inside a CAPTURES body, walk top-level entries. Three shapes show up in
/// the codebase:
///
/// - `(v1, v2, "path")`        — anonymous tuple literals
/// - `capture!(v1, "path")`    — `capture!` macro invocations
/// - `TypeName { f1: v1, .. }` — named struct literals (e.g. `ScrDiCapture { ... }`)
///
/// Returns the inner content of each entry (between the matching opener and
/// closer) regardless of brace style. Comments are not stripped — string
/// literal extraction below ignores them by construction.
fn read_capture_entries(body: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut cursor = 0usize;
    let bytes = body.as_bytes();
    while cursor < bytes.len() {
        // Find the next `(` or `{` that opens a top-level entry.
        let mut opener: Option<u8> = None;
        while cursor < bytes.len() {
            let byte = bytes[cursor];
            if byte == b'(' || byte == b'{' {
                opener = Some(byte);
                break;
            }
            cursor += 1;
        }
        let Some(open_byte) = opener else {
            break;
        };
        let close_byte = if open_byte == b'(' { b')' } else { b'}' };
        let inner_start = cursor + 1;
        let mut depth = 1usize;
        let mut scan = inner_start;
        while scan < bytes.len() && depth > 0 {
            let byte = bytes[scan];
            // String literals can contain `()` / `{}` — skip past them so
            // their contents don't perturb the depth counter.
            if byte == b'"' {
                scan += 1;
                let mut escaped = false;
                while scan < bytes.len() {
                    let inner_byte = bytes[scan];
                    if escaped {
                        escaped = false;
                    } else if inner_byte == b'\\' {
                        escaped = true;
                    } else if inner_byte == b'"' {
                        break;
                    }
                    scan += 1;
                }
                scan += 1;
                continue;
            }
            if byte == open_byte {
                depth += 1;
            } else if byte == close_byte {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            scan += 1;
        }
        if depth != 0 {
            break;
        }
        entries.push(&body[inner_start..scan]);
        cursor = scan + 1;
    }
    entries
}

/// Either a string literal or a numeric literal, in the order they appear
/// inside an argument list. Captures pair their parameter values with their
/// file path; some sources use string-valued parameters (`capture("ah", "path")`),
/// others numeric (`capture(25, "path")`), so we surface both.
#[derive(Debug, Clone)]
enum Literal {
    String(String),
    Number(f64),
}

/// Pull every double-quoted literal out of an arg list.
fn read_string_literals_in(args: &str) -> Vec<String> {
    read_literals_in(args)
        .into_iter()
        .filter_map(|literal| match literal {
            Literal::String(value) => Some(value),
            Literal::Number(_) => None,
        })
        .collect()
}

/// Pull both string and numeric literals from an arg list, in source order.
fn read_literals_in(args: &str) -> Vec<Literal> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let bytes = args.as_bytes();
    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte == b'"' {
            let start = cursor + 1;
            let mut scan = start;
            let mut escaped = false;
            while scan < bytes.len() {
                let inner = bytes[scan];
                if escaped {
                    escaped = false;
                } else if inner == b'\\' {
                    escaped = true;
                } else if inner == b'"' {
                    break;
                }
                scan += 1;
            }
            if scan >= bytes.len() {
                break;
            }
            out.push(Literal::String(args[start..scan].to_string()));
            cursor = scan + 1;
            continue;
        }
        // Numeric literal: optional sign, digits, optional `.digits`.
        let is_digit = byte.is_ascii_digit();
        let is_negative = byte == b'-' && cursor + 1 < bytes.len() && bytes[cursor + 1].is_ascii_digit();
        if is_digit || is_negative {
            // Reject identifiers that happen to start with digits — only
            // accept when preceded by whitespace, comma, `(`, `[`, `,`.
            let prev = if cursor == 0 {
                None
            } else {
                Some(bytes[cursor - 1])
            };
            let valid_prefix = prev
                .map(|prev| matches!(prev, b'(' | b',' | b' ' | b'\n' | b'\t' | b'[' | b'='))
                .unwrap_or(true);
            if !valid_prefix {
                cursor += 1;
                continue;
            }
            let start = cursor;
            let mut scan = start;
            if bytes[scan] == b'-' {
                scan += 1;
            }
            while scan < bytes.len() && bytes[scan].is_ascii_digit() {
                scan += 1;
            }
            if scan < bytes.len() && bytes[scan] == b'.' {
                scan += 1;
                while scan < bytes.len() && bytes[scan].is_ascii_digit() {
                    scan += 1;
                }
            }
            if let Ok(value) = args[start..scan].parse::<f64>() {
                out.push(Literal::Number(value));
            }
            cursor = scan;
            continue;
        }
        cursor += 1;
    }
    out
}

/// Walk the source for `enum_parameter(...)` and `float_parameter(...)`
/// invocations and return one [`GridParameter`] per call, in source order.
///
/// `enum_parameter(name, display, group, default, &[(v, l), ...])` lists its
/// values explicitly. `float_parameter(name, display, group, default, min,
/// max, step, unit)` doesn't — we materialize the discrete points by
/// stepping `min..=max` by `step`, which is what the hosting NAM/IR
/// captures expect.
fn read_enum_parameters(source: &str) -> Vec<GridParameter> {
    let mut parameters = Vec::new();
    for invocation in find_function_invocations(source, &["enum_parameter", "float_parameter"]) {
        let args = invocation.args;
        let literals = read_literals_in(args);
        let strings: Vec<&String> = literals
            .iter()
            .filter_map(|literal| match literal {
                Literal::String(value) => Some(value),
                Literal::Number(_) => None,
            })
            .collect();
        if strings.len() < 2 {
            continue;
        }
        let name = strings[0].clone();
        let display = strings[1].clone();

        let values = match invocation.callee {
            "enum_parameter" => {
                let Some(slice_offset) = args.find("&[") else {
                    continue;
                };
                let slice_segment = &args[slice_offset..];
                let slice_literals = read_string_literals_in(slice_segment);
                if slice_literals.is_empty() {
                    continue;
                }
                slice_literals
                    .into_iter()
                    .step_by(2)
                    .map(ParameterValue::Text)
                    .collect()
            }
            "float_parameter" => {
                // Numeric literals, in order: default (Some(_)), min, max, step.
                let numbers: Vec<f64> = literals
                    .iter()
                    .filter_map(|literal| match literal {
                        Literal::Number(value) => Some(*value),
                        Literal::String(_) => None,
                    })
                    .collect();
                if numbers.len() < 4 {
                    continue;
                }
                let min = numbers[1];
                let max = numbers[2];
                let step = numbers[3];
                materialize_discrete_range(min, max, step)
                    .into_iter()
                    .map(ParameterValue::Number)
                    .collect()
            }
            _ => continue,
        };

        parameters.push(GridParameter {
            name,
            display_name: Some(display),
            values,
        });
    }
    parameters
}

struct Invocation<'a> {
    callee: &'a str,
    args: &'a str,
}

/// Find every top-level invocation `name(...)` in the source for any of the
/// callees in `names`. Returns the inner argument string of each match in
/// source order.
fn find_function_invocations<'a>(source: &'a str, names: &[&'a str]) -> Vec<Invocation<'a>> {
    let mut result = Vec::new();
    for name in names {
        let needle = format!("{name}(");
        let mut cursor = 0usize;
        while let Some(found) = source[cursor..].find(&needle) {
            let arg_start = cursor + found + needle.len();
            let bytes = source.as_bytes();
            let mut depth = 1usize;
            let mut scan = arg_start;
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
            result.push(Invocation {
                callee: name,
                args: &source[arg_start..scan],
            });
            cursor = scan + 1;
        }
    }
    result
}

fn materialize_discrete_range(min: f64, max: f64, step: f64) -> Vec<f64> {
    if step <= 0.0 || max < min {
        return vec![min];
    }
    let mut values = Vec::new();
    let mut current = min;
    // Floating-point safety: stop when we've passed max by more than half a
    // step, and round each point to the nearest 1e-6 to keep the YAML clean.
    while current <= max + step * 0.5 {
        let rounded = (current * 1e6).round() / 1e6;
        values.push(rounded);
        current += step;
    }
    values
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

    // Prefer the `const CAPTURES: &[T] = &[...]` array when present — it
    // carries the full grid with parameter values. Only fall back to the
    // const-CAPTURE_<NAME> pattern (single or multi flat captures, no
    // parameters) when the array literal is genuinely absent.
    if let Some(captures_body) = read_captures_block(source) {
        let entries = read_capture_entries(captures_body);
        return build_grid_manifest_from_entries(
            model_id,
            display_name,
            brand,
            block_type,
            flavor,
            parameters,
            entries,
        );
    }

    let const_captures = scan_const_capture_paths(source);
    if !const_captures.is_empty() {
        let captures: Vec<GridCapture> = const_captures
            .into_iter()
            .map(|raw| GridCapture {
                values: BTreeMap::new(),
                file: PathBuf::from(strip_path_prefix(&raw, flavor)),
            })
            .collect();
        let backend = match flavor {
            "nam" => Backend::Nam {
                parameters: vec![],
                captures,
            },
            "ir" => Backend::Ir {
                parameters: vec![],
                captures,
            },
            other => return Err(anyhow!("unknown grid flavor `{other}`")),
        };
        return Ok(PluginManifest {
            manifest_version: 1,
            id: model_id.to_string(),
            display_name: display_name.to_string(),
            author: None,
            description: None,
            inspired_by: brand.map(str::to_string),
            brand: None,
            thumbnail: None,
            photo: None,
            screenshot: None,
            brand_logo: None,
            license: None,
            homepage: None,
            block_type,
            backend,
        });
    }

    Err(anyhow!("no captures found (neither array nor `const CAPTURE_*`)"))
}

fn build_grid_manifest_from_entries(
    model_id: &str,
    display_name: &str,
    brand: Option<&str>,
    block_type: BlockType,
    flavor: &str,
    parameters: Vec<GridParameter>,
    entries: Vec<&str>,
) -> Result<PluginManifest> {

    // For NAM/IR captures, the file path is the LAST string literal in each
    // entry; the parameter values precede it. We use the parameter list to
    // know how many values to bind by position.
    let mut captures: Vec<GridCapture> = Vec::new();
    for entry in entries {
        let literals = read_literals_in(entry);
        if literals.is_empty() {
            continue;
        }
        // The last string literal is always the asset path. Everything
        // before that — strings or numbers, in source order — feeds the
        // declared parameters by position.
        let last_string_index = literals
            .iter()
            .rposition(|literal| matches!(literal, Literal::String(_)));
        let Some(file_index) = last_string_index else {
            continue;
        };
        let file_relative = match &literals[file_index] {
            Literal::String(value) => value.clone(),
            Literal::Number(_) => continue,
        };
        let value_literals = &literals[..file_index];
        let mut values = BTreeMap::new();
        for (parameter, literal) in parameters.iter().zip(value_literals.iter()) {
            let parameter_value = match literal {
                Literal::String(text) => ParameterValue::Text(text.clone()),
                Literal::Number(number) => ParameterValue::Number(*number),
            };
            values.insert(parameter.name.clone(), parameter_value);
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
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        block_type,
        backend,
    })
}

/// Pull every `[pub] const CAPTURE_<NAME>: &str = "..."` value out of the
/// source, in source order. Used by single-capture and multi-named-capture
/// plugins (e.g. `pot_boost` with one `CAPTURE_PATH`, or `vox_ac30` with
/// `CAPTURE_STANDARD` and `CAPTURE_CLEAN_65PRINCE`).
fn scan_const_capture_paths(source: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = source[cursor..].find("const CAPTURE") {
        let abs = cursor + offset;
        let line_end = source[abs..]
            .find('\n')
            .map(|relative| abs + relative)
            .unwrap_or(source.len());
        // The `const CAPTURE...` line may continue onto the next line if
        // the literal is wrapped, so scan from the const start until we
        // find a `"` and read it as a string literal.
        let after = &source[abs..];
        let needle_eq = after.find('=');
        if let Some(eq_offset) = needle_eq {
            let from_eq = &after[eq_offset..];
            if let Some(quote_offset) = from_eq.find('"') {
                if let Some(value) = read_string_literal(&from_eq[quote_offset..]) {
                    found.push(value);
                }
            }
        }
        cursor = line_end + 1;
    }
    found
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
            let in_pkg = PathBuf::from("platform")
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
        brand: None,
        thumbnail: None,
        photo: None,
        screenshot: None,
        brand_logo: None,
        license: None,
        homepage: None,
        block_type,
        backend: Backend::Lv2 {
            plugin_uri,
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
    folder_id: &str,
    manifest: &PluginManifest,
    source_file: &Path,
    source_text: &str,
) -> Result<()> {
    let package_dir = out.join(folder_id);
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
        Backend::Lv2 { binaries, .. } => {
            // Copy TTLs from data/lv2/<dir>/ into each platform/<slot>/ dir
            let plugin_dir = read_str_const(source_text, "PLUGIN_DIR", false)
                .ok_or_else(|| anyhow!("PLUGIN_DIR missing for LV2 copy"))?;
            let ttl_source = PathBuf::from(LV2_DATA_ROOT).join(&plugin_dir);
            for (slot, in_pkg) in binaries {
                let dst = package_dir.join(in_pkg);
                let dst_parent = dst
                    .parent()
                    .ok_or_else(|| anyhow!("binary path has no parent"))?;
                fs::create_dir_all(dst_parent)?;
                if ttl_source.is_dir() {
                    for entry in fs::read_dir(&ttl_source)? {
                        let entry = entry?;
                        if entry.file_type()?.is_file() {
                            fs::copy(entry.path(), dst_parent.join(entry.file_name()))?;
                        }
                    }
                }
                let host_dir = host_dir_for_slot(slot);
                let filename = in_pkg
                    .file_name()
                    .ok_or_else(|| anyhow!("binary path has no filename"))?;
                let src = PathBuf::from(LV2_BIN_ROOT).join(host_dir).join(filename);
                fs::copy(&src, &dst).with_context(|| format!("copy {}", src.display()))?;
            }
        }
    }

    if let Some(thumb_dest) = &manifest.thumbnail {
        if let Some(src) = locate_thumbnail(&manifest.id, manifest.block_type) {
            let dst = package_dir.join(thumb_dest);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dst)?;
        }
    }
    if let Some(photo_dest) = &manifest.photo {
        if let Some(src) = locate_photo(&manifest.id) {
            let dst = package_dir.join(photo_dest);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dst)?;
        }
    }
    if let Some(screenshot_dest) = &manifest.screenshot {
        if let Some(src) = locate_screenshot(&manifest.id, manifest.block_type) {
            let dst = package_dir.join(screenshot_dest);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&src, &dst)?;
        }
    }
    if let Some(brand_logo_dest) = &manifest.brand_logo {
        if let Some(brand_value) = &manifest.brand {
            if let Some((src, _ext)) = locate_brand_logo(brand_value) {
                let dst = package_dir.join(brand_logo_dest);
                if let Some(parent) = dst.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&src, &dst)?;
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
    let basename = in_pkg_path
        .file_name()
        .ok_or_else(|| anyhow!("no basename for {}", in_pkg_path.display()))?
        .to_string_lossy()
        .to_string();
    // 1. Try the array literal path used by grid-style sources.
    if let Some(captures_body) = read_captures_block(source_text) {
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
    }
    // 2. Fall back to the const CAPTURE_<NAME> pattern.
    for raw in scan_const_capture_paths(source_text) {
        if Path::new(&raw)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            == Some(basename.clone())
        {
            return Ok(PathBuf::from(NAM_CAPTURES_ROOT).join(&raw));
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
    let mut paths: Vec<String> = Vec::new();
    if let Some(body) = read_captures_block(source_text) {
        for entry in read_capture_entries(body) {
            for literal in read_string_literals_in(entry) {
                paths.push(literal);
            }
        }
    }
    for raw in scan_const_capture_paths(source_text) {
        paths.push(raw);
    }
    for raw in paths {
        if Path::new(&raw)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            != Some(basename.clone())
        {
            continue;
        }
        let candidate = PathBuf::from(IR_CAPTURES_ROOT).join(&raw);
        if candidate.is_file() {
            return Ok(candidate);
        }
        // Fallback: source has stale `_<N>` suffix the real file lacks.
        // Strip any `_<digit>.wav` suffix and retry.
        if let Some(stripped) = strip_take_suffix(&raw) {
            let candidate_stripped = PathBuf::from(IR_CAPTURES_ROOT).join(&stripped);
            if candidate_stripped.is_file() {
                return Ok(candidate_stripped);
            }
        }
    }
    Err(anyhow!("could not resolve IR source for {basename}"))
}

/// Trim a single trailing `_<digits>` before the file extension. Lets us
/// match `foo.wav` from a stale `foo_3.wav` reference.
fn strip_take_suffix(path: &str) -> Option<String> {
    let (stem, ext) = path.rsplit_once('.')?;
    let (head, suffix) = stem.rsplit_once('_')?;
    if !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(format!("{head}.{ext}"))
}

fn copy_asset(src: &Path, dst: &Path) -> Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    Ok(())
}
