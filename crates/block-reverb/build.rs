use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let src_dir = manifest_dir.join("src");
    let mut model_modules = Vec::new();

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
        if contents.contains("MODEL_DEFINITION") {
            model_modules.push(stem.to_string());
        }
    }

    model_modules.sort();
    let mut generated = String::new();
    for module_name in &model_modules {
        generated.push_str(&format!("#[path = \"{}/{}.rs\"]\nmod {};\n", src_dir.to_string_lossy().replace("\\", "/"), module_name, module_name));
    }
    generated.push_str("\npub const SUPPORTED_MODELS: &[&str] = &[\n");
    for module_name in &model_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION.id,\n", module_name));
    }
    generated.push_str("];\n\nconst MODEL_DEFINITIONS: &[ReverbModelDefinition] = &[\n");
    for module_name in &model_modules {
        generated.push_str(&format!("    {}::MODEL_DEFINITION,\n", module_name));
    }
    generated.push_str("];\n");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    fs::write(out_dir.join("generated_registry.rs"), generated).expect("write registry");
}
