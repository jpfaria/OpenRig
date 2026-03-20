use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let src_dir = manifest_dir.join("src");
    let mut compressor_modules = Vec::new();
    let mut gate_modules = Vec::new();

    for entry in fs::read_dir(&src_dir).expect("read src dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let stem = path.file_stem().and_then(|stem| stem.to_str()).expect("file stem");
        if matches!(stem, "lib" | "registry") {
            continue;
        }
        let contents = fs::read_to_string(&path).expect("read source");
        if !contents.contains("MODEL_DEFINITION") {
            continue;
        }
        let item = (stem.to_string(), path.canonicalize().expect("canonical path"));
        if stem.starts_with("compressor_") {
            compressor_modules.push(item);
        } else if stem.starts_with("gate_") {
            gate_modules.push(item);
        }
    }

    compressor_modules.sort_by(|a, b| a.0.cmp(&b.0));
    gate_modules.sort_by(|a, b| a.0.cmp(&b.0));

    let mut generated = String::new();
    for (module_name, module_path) in compressor_modules.iter().chain(gate_modules.iter()) {
        generated.push_str(&format!("#[path = {:?}]\nmod {};\n", module_path.to_string_lossy().to_string(), module_name));
    }
    generated.push_str("\npub const COMPRESSOR_SUPPORTED_MODELS: &[&str] = &[\n");
    for (module_name, _) in &compressor_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION.id,\n", module_name));
    }
    generated.push_str("];\n\npub const GATE_SUPPORTED_MODELS: &[&str] = &[\n");
    for (module_name, _) in &gate_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION.id,\n", module_name));
    }
    generated.push_str("];\n\nconst COMPRESSOR_MODEL_DEFINITIONS: &[DynModelDefinition] = &[\n");
    for (module_name, _) in &compressor_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION,\n", module_name));
    }
    generated.push_str("];\n\nconst GATE_MODEL_DEFINITIONS: &[DynModelDefinition] = &[\n");
    for (module_name, _) in &gate_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION,\n", module_name));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    fs::write(out_dir.join("generated_registry.rs"), generated).expect("write registry");
}
