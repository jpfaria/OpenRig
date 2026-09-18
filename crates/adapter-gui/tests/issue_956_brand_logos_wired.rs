//! #956 — every logo shipped in `assets/brands/<brand>/` must be reachable
//! through `BrandLogo`: the brand shows up in `has-logo` and in the `source`
//! chain, and the file the chain points at exists. A logo added to assets but
//! never mapped is invisible in the app (mesa, orange, engl… sat unwired for
//! months).

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn brand_logo_source() -> String {
    std::fs::read_to_string(repo_root().join("crates/adapter-gui/ui/components/brand_logo.slint"))
        .expect("read brand_logo.slint")
}

fn shipped_brands() -> Vec<String> {
    let mut brands: Vec<String> = std::fs::read_dir(repo_root().join("assets/brands"))
        .expect("read assets/brands")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|brand| brand != "openrig")
        .collect();
    brands.sort();
    brands
}

#[test]
fn every_shipped_brand_logo_is_mapped_in_brand_logo() {
    let source = brand_logo_source();
    let unmapped: Vec<String> = shipped_brands()
        .into_iter()
        .filter(|brand| {
            let check = format!("root.brand == \"{brand}\"");
            let path = format!("assets/brands/{brand}/logo.");
            source.matches(&check).count() < 2 || !source.contains(&path)
        })
        .collect();
    assert!(
        unmapped.is_empty(),
        "logos shipped but not mapped: {unmapped:?}"
    );
}

#[test]
fn every_logo_the_mapping_points_at_exists() {
    let source = brand_logo_source();
    let missing: Vec<String> = source
        .split("@image-url(\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(|relative| {
            repo_root()
                .join("crates/adapter-gui/ui/components")
                .join(relative)
        })
        // guild/takamine shipped as 0-byte files and rendered nothing.
        .filter(|path| std::fs::metadata(path).map_or(true, |meta| meta.len() == 0))
        .map(|path| path.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "mapped logos missing on disk: {missing:?}"
    );
}
