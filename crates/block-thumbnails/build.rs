use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let thumbnails_dir = manifest_dir.join("../../assets/blocks/thumbnails");
    let thumbnails_dir = thumbnails_dir.canonicalize().expect("thumbnails dir not found");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let out_file = out_dir.join("generated_thumbnails.rs");

    println!("cargo:rerun-if-changed=../../assets/blocks/thumbnails");

    let mut entries: Vec<(String, String, PathBuf)> = Vec::new();

    let mut type_dirs: Vec<_> = fs::read_dir(&thumbnails_dir)
        .expect("failed to read thumbnails dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    type_dirs.sort_by_key(|e| e.file_name());

    for type_entry in type_dirs {
        let effect_type = type_entry.file_name().to_string_lossy().to_string();
        let type_path = type_entry.path();

        let mut files: Vec<_> = fs::read_dir(&type_path)
            .expect("failed to read type dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && e.path().extension().and_then(|x| x.to_str()) == Some("png")
            })
            .collect();
        files.sort_by_key(|e| e.file_name());

        for file_entry in files {
            let file_name = file_entry.file_name();
            let file_name_str = file_name.to_string_lossy();
            let model_id = file_name_str
                .strip_suffix(".png")
                .unwrap_or(&file_name_str)
                .to_string();
            let abs_path = file_entry.path();
            entries.push((effect_type.clone(), model_id, abs_path));
        }
    }

    let mut f = fs::File::create(&out_file).expect("failed to create generated_thumbnails.rs");

    writeln!(f, "const THUMBNAILS: &[(&str, &str, &[u8])] = &[").unwrap();
    for (effect_type, model_id, abs_path) in &entries {
        writeln!(
            f,
            "    (\"{effect_type}\", \"{model_id}\", include_bytes!(\"{}\")),",
            abs_path.display()
        )
        .unwrap();
    }
    writeln!(f, "];").unwrap();
}
