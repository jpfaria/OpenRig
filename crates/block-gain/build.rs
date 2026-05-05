use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let src_dir = manifest_dir.join("src");
    let lv2_libs_dir = lv2_libs_dir(&manifest_dir);
    let mut model_modules = Vec::new();
    let mut available_modules = Vec::new();
    let mut thumbnail_modules = Vec::new();

    for entry in fs::read_dir(&src_dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str()).expect("file stem");
        if matches!(stem, "lib" | "registry") {
            continue;
        }
        if stem.ends_with("_tests") {
            continue;
        }
        let contents = fs::read_to_string(&path).expect("read source");
        if !contents.contains("MODEL_DEFINITION") {
            continue;
        }
        model_modules.push(stem.to_string());
        if plugin_binary_present(&contents, &lv2_libs_dir) {
            available_modules.push(stem.to_string());
        }
        if has_thumbnail(&contents) {
            thumbnail_modules.push(stem.to_string());
        }
    }

    model_modules.sort();
    available_modules.sort();
    thumbnail_modules.sort();
    let mut generated = String::new();
    for module_name in &model_modules {
        generated.push_str(&format!("#[path = \"{}/{}.rs\"]\nmod {};\n", src_dir.to_string_lossy().replace("\\", "/"), module_name, module_name));
    }
    generated.push_str("\npub const SUPPORTED_MODELS: &[&str] = &[\n");
    for module_name in &model_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION.id,\n", module_name));
    }
    generated.push_str("];\n\npub const AVAILABLE_MODEL_IDS: &[&str] = &[\n");
    for module_name in &available_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION.id,\n", module_name));
    }
    generated.push_str("];\n\nconst MODEL_DEFINITIONS: &[GainModelDefinition] = &[\n");
    for module_name in &model_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION,\n", module_name));
    }
    generated.push_str("];\n\npub const THUMBNAILS: &[(&str, &str)] = &[\n");
    for module_name in &thumbnail_modules {
        generated.push_str(&format!(
            "    ({}::MODEL_ID, match {}::THUMBNAIL_PATH {{ Some(p) => p, None => \"\" }}),\n",
            module_name, module_name
        ));
    }
    generated.push_str("];\n");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    fs::write(out_dir.join("generated_registry.rs"), generated).expect("write registry");
}

fn lv2_libs_dir(manifest_dir: &Path) -> PathBuf {
    let project_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.to_path_buf());
    project_root.join("libs").join("lv2").join(platform_dir())
}

fn platform_dir() -> &'static str {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    match (os.as_str(), arch.as_str()) {
        ("macos", _) => "macos-universal",
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-aarch64",
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        _ => "macos-universal",
    }
}

/// Read PLUGIN_BINARY for the current target_os out of an LV2 wrapper source
/// file and check that the binary is present in libs/lv2/<platform>/.
///
/// Returns `true` for any file that does NOT declare a PLUGIN_BINARY const at
/// all (manual files with their own resolution logic) — those are trusted.
fn plugin_binary_present(source: &str, libs_dir: &Path) -> bool {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let cfg_marker = match os.as_str() {
        "macos" => "#[cfg(target_os = \"macos\")]",
        "linux" => "#[cfg(target_os = \"linux\")]",
        "windows" => "#[cfg(target_os = \"windows\")]",
        _ => return true,
    };
    let mut has_plugin_binary_const = false;
    let mut next_is_plugin_binary = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed == cfg_marker {
            next_is_plugin_binary = true;
            continue;
        }
        if next_is_plugin_binary {
            next_is_plugin_binary = false;
            if let Some(name) = parse_plugin_binary_const(trimmed) {
                has_plugin_binary_const = true;
                if libs_dir.join(&name).exists() {
                    return true;
                }
            }
        } else if trimmed.contains("const PLUGIN_BINARY") {
            has_plugin_binary_const = true;
        }
    }
    !has_plugin_binary_const
}

fn parse_plugin_binary_const(line: &str) -> Option<String> {
    let prefix = "const PLUGIN_BINARY: &str = \"";
    let rest = line.strip_prefix(prefix)?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn has_thumbnail(source: &str) -> bool {
    source.contains("pub const THUMBNAIL_PATH: Option<&str> = Some(")
}
